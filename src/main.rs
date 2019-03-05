#[macro_use]
extern crate clap;
extern crate reqwest;
#[cfg_attr(test, macro_use)]
extern crate serde_json;
extern crate scoped_threadpool;

use clap::{App, Arg};
use serde_json::value::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs::{File, rename};
use std::io::BufWriter;
use std::io::{copy, Read};
use std::path::{Path, PathBuf};
use std::process;

static TREEHERDER_BASE: &str = "https://treeherder.mozilla.org";

arg_enum!{
    #[derive(Clone, Debug)]
    pub enum LogType {
        Raw,
        WptReport
    }
}

fn parse_args<'a, 'b>() -> App<'a, 'b> {
    App::new("Treeherder log fetcher")
        .arg(Arg::with_name("check_complete")
             .long("--check-complete")
             .required(false)
             .help("Check if there are any pending wpt jobs and exit with code 1 if there are"))
        .arg(Arg::with_name("out_dir")
             .long("--out-dir")
             .takes_value(true)
             .required(false)
             .help("Directory in which to put output files"))
        .arg(Arg::with_name("log_type")
             .long("--log-type")
             .possible_values(&["raw", "wptreport"])
             .default_value("raw")
             .takes_value(true)
             .help("Log type to fetch. raw or wptreport"))
        .arg(Arg::with_name("branch")
             .required(true)
             .index(1)
             .help("Branch on which jobs ran"))
        .arg(Arg::with_name("commit")
             .required(true)
             .index(2)
             .help("Commit hash for push"))
}

fn get_json(client: &reqwest::Client, url: &str, body: Option<&BTreeMap<&str, Value>>) -> reqwest::Result<Value> {
    let mut req = client.get(url);
    if let Some(body) = body {
        req = req.json(body)
    };
    let mut resp = req.send()?;
    resp.error_for_status_ref()?;
    let mut resp_body = String::new();
    resp.read_to_string(&mut resp_body).unwrap();
    let data: Value = serde_json::from_str(&*resp_body).unwrap();
    Ok(data)
}

fn th_url(path: String) -> String {
    format!("{}{}", TREEHERDER_BASE, path).into()
}

fn commit_is_valid(commit: &str) -> bool {
    if commit.len() < 12 || commit.len() > 40 {
        return false;
    }
    return true;
}

fn get_result_set(client: &reqwest::Client, branch: &str, commit: &str) -> reqwest::Result<u64> {
    let body = BTreeMap::new();
    let data = get_json(client, &*th_url(format!("/api/project/{}/push/?revision={}", branch, commit)), Some(&body))?;

    Ok(data.pointer("/results/0/id")
       .and_then(|x| x.as_u64())
       .unwrap())
}

fn get_jobs(client: &reqwest::Client, branch: &str, result_set_id: u64, state: Option<String>) -> reqwest::Result<Vec<Value>> {
    let body = BTreeMap::new();
    let mut url = format!("/api/project/{}/jobs/?result_set_id={}&count=2000&exclusion_profile=false", branch, result_set_id);
    if let Some(state) = state {
        url = format!("{}&state={}", url, state)
    }
    let data = get_json(client, &*th_url(url), Some(&body))?;
    Ok(data.pointer("/results").and_then(|x| x.as_array()).map(|x| x.clone()).unwrap())
}

fn get_log_url(client: &reqwest::Client, job_guid: &str, name: &str) -> Option<String> {
    let body = BTreeMap::new();

    get_json(client, &*th_url(format!("/api/jobdetail/?job_guid={}", job_guid)), Some(&body))
        .ok()
        .and_then(|x| x.get("results")
                  .and_then(|x|x.as_array())
                  .and_then(|x| x.iter()
                            .find(|x| x.get("value")
                                  .and_then(|x| x.as_str())
                                  .map(|x| x == name)
                                  .unwrap_or(false)))
                  .and_then(|x| x
                            .get("url")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())))
}

fn download(client: &reqwest::Client, out_dir: &Path, name: &Path, url: &str) {
    let tmp_name = out_dir.join(name.with_extension("tmp"));
    let mut dest = BufWriter::new(File::create(&tmp_name).unwrap());
    let mut resp = client.get(url).send().unwrap();
    copy(&mut resp, &mut dest).unwrap();
    rename(&tmp_name, out_dir.join(name)).unwrap();
}

fn filter_wpt_job(job: &Value) -> bool {
    let name = job.get("job_type_name")
        .and_then(|x| x.as_str());
    if name.is_none() {
        return false
    }
    let name = name.expect("Invariant: name is not None");
    if !(name.starts_with("W3C Web Platform") ||
         (name.starts_with("test-") &&
          name.contains("-web-platform-tests-"))) {
        return false
    }
    return true
}

fn fetch_job_logs(client: &reqwest::Client, out_dir: &Path, jobs: Vec<Value>, log_type: LogType) {
    let mut pool = scoped_threadpool::Pool::new(8);
    let file_name = match log_type {
        LogType::Raw => "wpt_raw.log",
        LogType::WptReport => "wptreport.json"
    };
    pool.scoped(|scope| {
        for job in jobs.iter().filter(|e| filter_wpt_job(e)) {
            let job_guid = job.get("job_guid")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string()) // Seems we borrow for the entire |scope|
                .expect("Invariant: job_guid is not None");
            let client = client.clone();
            let platform = job.get("platform")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string())
                .expect("Invariant: platform must be defined for job");
            let name = PathBuf::from(format!("{}-{}.log", platform, job_guid.replace("/", "-")));
            if !name.exists() {
                scope.execute(move || {
                    let log_url = get_log_url(&client, &*job_guid, file_name);
                    println!("{} {} {:?}", platform, job_guid, log_url);
                    if let Some(url) = log_url {
                        download(&client, out_dir, &name, &*url);
                    }
                });
            }
        }
    })
}

fn get_decision(client: &reqwest::Client, branch: &str, result_set_id: u64) -> reqwest::Result<Value> {
    let url = th_url(format!("/api/project/{}/jobs/?result_set_id={}&count=10&exclusion_profile=false&job_type_name=Gecko+Decision+Task", branch, result_set_id));
    let data = get_json(client, &url, None)?;
    let results = data.pointer("/results").and_then(|x| x.as_array()).unwrap();
    if results.len() > 1 {
        panic!("Got multiple decision tasks");
    }
    Ok(results.get(0).expect("No decision task found").clone())
}

fn get_task_graph(client: &reqwest::Client, job_guid: &str) -> reqwest::Result<Value> {
    let task_graph_url = get_log_url(client, job_guid, "task-graph.json").unwrap();
    get_json(client, &task_graph_url, None)
}

fn wpt_complete(client: &reqwest::Client, branch: &str, result_set_id: u64) -> bool {
    // This is assuming that the job has at least started
    let decision_result = get_decision(client, branch, result_set_id);
    if decision_result.is_err() {
        println!("No decision task found");
        return false
    }
    let decision = decision_result.unwrap();
    if !decision.get("state").map(|x| x == "completed").unwrap_or(false) {
        println!("Decision task not complete");
        return false
    }
    let job_guid = decision.get("job_guid").and_then(|x| x.as_str()).expect("Invariant: Decision task must have a job_guid");
    if let Ok(task_graph) = get_task_graph(client, job_guid) {
        if task_graph.as_object().unwrap().iter()
            .filter(|&(_, task)| {
                task.pointer("/attributes/unittest_suite").map(|x| x == "web-platform-tests").unwrap_or(false)
            }).count() == 0 {
                // Task is trivially done since there are no wpt jobs (assuming everything is on TC)
                println!("No wpt jobs scheduled");
                return true
            }
    } else {
        println!("Failed to fetch task graph");
        return false
    }

    let pending_jobs = get_jobs(client, branch, result_set_id, Some("pending".into())).unwrap();
    if pending_jobs.iter().filter(|e| filter_wpt_job(e)).count() > 0 {
        println!("wpt jobs still pending");
        return false
    }
    true
}

fn main() {
    let matches = parse_args().get_matches();
    let branch = matches.value_of("branch").unwrap();
    let commit = matches.value_of("commit").unwrap();
    let log_type = value_t_or_exit!(matches, "log_type", LogType);

    if !commit_is_valid(&commit) {
        println!("Commit `{}` needs to be between 12 and 40 characters in length", commit);
        process::exit(1);
    }

    let cur_dir = env::current_dir().expect("Invalid working directory");
    let out_dir: PathBuf = if let Some(dir) = matches.value_of("out_dir") {
        cur_dir.join(dir)
    } else {
        cur_dir
    };
    if !out_dir.is_dir() {
        println!("{} is not a directory", out_dir.display());
        process::exit(1);
    }

    let client = reqwest::Client::new();
    let result_set_id = get_result_set(&client, branch, commit).unwrap();
    if matches.is_present("check_complete") {
        if !(wpt_complete(&client, branch, result_set_id)) {
            process::exit(1);
        }
    }
    let jobs = get_jobs(&client, branch, result_set_id, Some("completed".into())).unwrap();
    fetch_job_logs(&client, &out_dir, jobs, log_type);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_wpt_job() {
        // job1 is a slimmed down version of a decision task, filter_wpt_job
        let job1 = json!({
            "job_type_id":6689,
            "job_type_name":"Gecko Decision Task",
            "job_type_symbol":"D",
            "who":"user@email.com"
        });

        // job2 is a slimmed down version of a wpt test
        let job2 = json!({
            "job_type_id":105958,
            "job_type_name":"test-linux64-qr/opt-web-platform-tests-reftests-e10s-2",
            "job_type_symbol":"Wr2",
            "who":"user@email.com"
        });
        assert_eq!(false, filter_wpt_job(&job1), "Make sure non-wpt jobs are filtered out");
        assert_eq!(true, filter_wpt_job(&job2), "Make sure wpt jobs are not filtered out");
    }

    #[test]
    fn test_th_url() {
        assert_eq!("https://treeherder.mozilla.org/api/project/try/resultset/?revision=1234567890ab",
                   th_url(format!("/api/project/{}/resultset/?revision={}", "try", "1234567890ab")));
    }
}
