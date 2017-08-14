extern crate clap;
extern crate reqwest;
extern crate serde_json;
extern crate scoped_threadpool;

use std::fs::{File, rename};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use std::io::{copy, Read};
use clap::{App, Arg};
use serde_json::value::Value;

static TREEHERDER_BASE: &str = "https://treeherder.mozilla.org";

fn parse_args<'a, 'b>() -> App<'a, 'b> {
    App::new("Treeherder log fetcher")
        .arg(Arg::with_name("branch")
             .required(true)
             .index(1)
             .help("Branch on which jobs ran"))
        .arg(Arg::with_name("commit")
             .required(true)
             .index(2)
             .help("Commit hash for push"))
}

fn get_json(client: &reqwest::Client, path: &str, body: &BTreeMap<&str, Value>) -> reqwest::Result<Value> {
    let abs_url = format!("{}{}", TREEHERDER_BASE, path);
    let mut resp = client.get(&*abs_url)
        .json(body)
        .send()?;
    check_status(&resp)?;
    let mut resp_body = String::new();
    resp.read_to_string(&mut resp_body).unwrap();
    let data: Value = serde_json::from_str(&*resp_body)?;
    Ok(data)
}

fn check_status(resp: &reqwest::Response) -> reqwest::Result<()> {
    if resp.status().is_success() {
        return Ok(())
    }
    Err(reqwest::Error::Http(reqwest::HyperError::Status))
}

fn get_result_set(client: &reqwest::Client, branch: &str, commit: &str) -> reqwest::Result<u64> {
    let body = BTreeMap::new();
    let data = get_json(client, &*format!("/api/project/{}/resultset/?revision={}", branch, commit), &body)?;

    Ok(data.pointer("/results/0/id")
       .and_then(|x| x.as_u64())
       .unwrap())
}

fn get_jobs(client: &reqwest::Client, branch: &str, result_set_id: u64) -> reqwest::Result<Vec<Value>> {
    let body = BTreeMap::new();
    let data = get_json(client, &*format!("/api/project/{}/jobs/?result_set_id={}&count=2000&exclusion_profile=false", branch, result_set_id), &body)?;
    Ok(data.pointer("/results").and_then(|x| x.as_array()).map(|x| x.clone()).unwrap())
}

fn get_log_url(client: &reqwest::Client, job_guid: &str) -> Option<String> {
    let body = BTreeMap::new();

    get_json(client, &*format!("/api/jobdetail/?job_guid={}", job_guid), &body)
        .ok()
        .and_then(|x| x.get("results")
                  .and_then(|x|x.as_array())
                  .and_then(|x| x.iter()
                            .find(|x| x.get("value")
                                  .and_then(|x| x.as_str())
                                  .map(|x| x == "wpt_raw.log")
                                  .unwrap_or(false)))
                  .and_then(|x| x
                            .get("url")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())))
}

fn download(client: &reqwest::Client, name: &Path, url: &str) {
    let tmp_name = name.with_extension("tmp");
    let mut dest = BufWriter::new(File::create(&tmp_name).unwrap());
    let mut resp = client.get(url).send().unwrap();
    copy(&mut resp, &mut dest).unwrap();
    rename(&tmp_name, &name).unwrap();
}

fn fetch_job_logs(client: &reqwest::Client, jobs: Vec<Value>) {
    let mut pool = scoped_threadpool::Pool::new(8);

    pool.scoped(|scope| {
        for job in jobs {
            let name = job.get("job_type_name")
                .and_then(|x| x.as_str());
            if name.is_none() {
                continue
            }
            let name = name.expect("Invariant: name is not None");
            if !(name.starts_with("W3C Web Platform") ||
                 (name.starts_with("test-") &&
                  name.contains("-web-platform-tests-"))) {
                continue
            }
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
                    let log_url = get_log_url(&client, &*job_guid);
                    println!("{} {} {:?}", platform, job_guid, log_url);
                    if let Some(url) = log_url {
                        download(&client, &name, &*url);
                    }
                });
            }
        }
    })
}

fn main() {
    let matches = parse_args().get_matches();
    let branch = matches.value_of("branch").unwrap();
    let commit = matches.value_of("commit").unwrap();
    let client = reqwest::Client::new().unwrap();
    let result_set_id = get_result_set(&client, branch, commit).unwrap();
    let jobs = get_jobs(&client, branch, result_set_id).unwrap();
    fetch_job_logs(&client, jobs);
}
