#[macro_use]
extern crate clap;
extern crate reqwest;
extern crate scoped_threadpool;
use serde::de::{DeserializeOwned};
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use clap::{App, Arg};
use std::env;
use std::fs::{rename, File};
use std::io::{self, copy, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::process;

arg_enum! {
    #[derive(Clone, Copy, Debug)]
    enum LogFormat {
        Raw,
        WptReport,
    }
}

fn parse_args<'a, 'b>() -> App<'a, 'b> {
    App::new("Treeherder log fetcher")
        .arg(
            Arg::with_name("check_complete")
                .long("--check-complete")
                .required(false)
                .help("Check if there are any pending wpt jobs and exit with code 1 if there are"),
        )
        .arg(
            Arg::with_name("out_dir")
                .long("--out-dir")
                .takes_value(true)
                .required(false)
                .help("Directory in which to put output files"),
        )
        .arg(
            Arg::with_name("log_format")
                .long("--log-format")
                .possible_values(&["raw", "wptreport"])
                .default_value("wptreport")
                .takes_value(true)
                .help("Log type to fetch. raw or wptreport"),
        )
        .arg(
            Arg::with_name("taskcluster_url")
                .long("--taskcluster-url")
                .default_value("https://firefox-ci-tc.services.mozilla.com")
                .takes_value(true)
                .help("Base url of the taskcluster instance"),
        )
        .arg(
            Arg::with_name("branch")
                .required(true)
                .index(1)
                .help("Branch on which jobs ran"),
        )
        .arg(
            Arg::with_name("commit")
                .required(true)
                .index(2)
                .help("Commit hash for push"),
        )
}

#[derive(Debug, Deserialize)]
struct RevisionResponse {
    node: String,
    desc: String,
    user: String,
    parents: Vec<String>,
    phase: String,
    pushid: u64,
    pushuser: String,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TaskState {
    Unscheduled,
    Pending,
    Running,
    Completed,
    Failed,
    Exception,
}

impl TaskState {
    fn is_complete(&self) -> bool {
        match self {
            TaskState::Unscheduled | TaskState::Pending | TaskState::Running => false,
            TaskState::Completed | TaskState::Failed | TaskState::Exception => true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct IndexResponse {
    namespace: String,
    taskId: String,
    rank: u64,
    expires: String
}


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Artifact {
    storageType: String,
    name: String,
    expires: String,
    contentType: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct TaskGroupResponse {
    taskGroupId: String,
    tasks: Vec<TaskGroupTask>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct TaskGroupTask {
    status: TaskGroupTaskStatus,
    task: Task,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct TaskGroupTaskStatus {
    taskId: String,
    provisionerId: String,
    workerType: String,
    schedulerId: String,
    taskGroupId: String,
    deadline: String, // Should be a time type
    expires: String,  // Should be a time type
    retriesLeft: u64,
    state: TaskState,
    runs: Vec<TaskRun>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct TaskRun {
    runId: u64,
    state: TaskState,
    reasonCreated: String,  // Should be an enum
    reasonResolved: Option<String>, // Should be an enum
    workerGroup: Option<String>,
    workerId: Option<String>,
    takenUntil: Option<String>, // Should be a time type
    scheduled: Option<String>,  // Should be a time type
    started: Option<String>,    // Should be a time type
    resolved: Option<String>,   // Should be a time type
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Task {
    provisionerId: String,
    workerType: String,
    schedulerId: String,
    taskGroupId: String,
    metadata: TaskMetadata, // Blah
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct TaskMetadata {
    owner: String,
    source: String,
    description: String,
    name: String,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
enum Error {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    Io(io::Error),
    String(String)
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        Error::Reqwest(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Error {
        Error::Serde(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

impl LogFormat {
    fn path_suffix(&self) -> String {
        format!("/{}", self.file_name())
    }

    fn file_name(&self) -> &'static str {
        match self {
            LogFormat::Raw => "wpt_raw.log",
            LogFormat::WptReport => "wptreport.json",
        }
    }
}

fn get_json<T>(client: &reqwest::Client, url: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    // TODO - If there's a list then support continuationToken
    println!("{}", url);
    let req = client.get(url);
    let mut resp = req.send()?;
    resp.error_for_status_ref()?;
    let mut resp_body = match resp.content_length() {
        Some(len) => String::with_capacity(len as usize),
        None => String::new()
    };
    resp.read_to_string(&mut resp_body)?;
    let data: T = serde_json::from_str(&resp_body)?;
    Ok(data)
}

fn url(base: &str, path: &str) -> String {
    format!("{}{}", base, path).into()
}

fn commit_is_valid(commit: &str) -> bool {
    if commit.len() < 12 || commit.len() > 40 {
        return false;
    }
    return true;
}

fn hg_path(branch: &str) -> &'static str {
    match branch {
        "try" => "try",
        "mozilla-beta" => "releases/mozilla-beta",
        "mozilla-central" => "mozilla-central",
        "mozilla-inbound" => "integration/mozilla-inbound",
        "autoland" => "integration/autoland",
        _ => panic!(format!("Unknown branch {}", branch))
    }
}

fn get_revision(client: &reqwest::Client, branch: &str, commit: &str) -> Result<RevisionResponse> {
    let url_ = format!("https://hg.mozilla.org/{}/json-rev/{}", hg_path(branch), commit);

    Ok(get_json(client, &url_)?)
}

fn get_taskgroup(
    client: &reqwest::Client,
    taskcluster_urls: &TaskclusterUrls,
    branch: &str,
    commit: &str,
) -> Result<IndexResponse> {
    let index = format!("gecko.v2.{}.revision.{}.firefox.decision", branch, commit);
    Ok(get_json(
        client,
        &url(&taskcluster_urls.index_base, &format!("task/{}", index)),
    )?)
}

fn get_taskgroup_tasks(client: &reqwest::Client, taskcluster_urls: &TaskclusterUrls, taskgroup_id: &str) -> Result<TaskGroupResponse> {
    let url_suffix = format!("task-group/{}/list", taskgroup_id);

    Ok(get_json(client, &url(&taskcluster_urls.queue_base, &url_suffix))?)
}

fn get_artifacts(client: &reqwest::Client, taskcluster_urls: &TaskclusterUrls, task_id: &str) -> Result<Vec<Artifact>> {
    let url_suffix = format!("task/{}/artifacts", task_id);
    let artifacts: ArtifactsResponse = get_json(client, &*url(&taskcluster_urls.queue_base, &url_suffix))?;
    Ok(artifacts.artifacts)
}

fn get_log_url(taskcluster_urls: &TaskclusterUrls, task_id: &str, artifact: &Artifact) -> String {
    let task_url = format!("task/{}/artifacts", task_id);
    url(&taskcluster_urls.queue_base, &format!("{}/{}", &task_url, artifact.name))
}

fn download(client: &reqwest::Client, name: &Path, url: &str) {
    let tmp_name = name.with_extension("tmp");
    let mut dest = BufWriter::new(File::create(&tmp_name).unwrap());
    let mut resp = client.get(url).send().unwrap();
    copy(&mut resp, &mut dest).unwrap();
    rename(&tmp_name, name).unwrap();
}

fn fetch_job_logs(
    client: &reqwest::Client,
    taskcluster_urls: &TaskclusterUrls,
    out_dir: &Path,
    tasks: Vec<TaskGroupTask>,
    log_format: LogFormat,
) {
    let mut pool = scoped_threadpool::Pool::new(8);
    pool.scoped(|scope| {
        for task in tasks {
            let task_id = task.status.taskId.clone();
            let client = client.clone();
            let name = PathBuf::from(format!(
                "{}-{}-{}",
                task.task.metadata.name.replace("/", "-"),
                &task.status.taskId,
                log_format.file_name()
            ));
            let dest = out_dir.join(&name);
            if !dest.exists() {
                scope.execute(move || {
                    let artifacts = get_artifacts(&client, &taskcluster_urls, &task_id).unwrap();
                    let artifact = artifacts
                        .iter()
                        .find(|&artifact| is_wpt_artifact(artifact, log_format));
                    if let Some(artifact) = artifact {
                        let log_url = get_log_url(&taskcluster_urls, &task_id, &artifact);
                        println!("Downloading {} to {}", log_url, dest.to_string_lossy());
                        download(&client, &dest, &log_url);
                    }
                });
            } else {
                println!("{} exists locally, skipping", dest.to_string_lossy());
            }
        }
    })
}

fn is_wpt_artifact(artifact: &Artifact, format: LogFormat) -> bool {
    return artifact.name.ends_with(&format.path_suffix());
}

fn is_wpt_task(task: &TaskGroupTask) -> bool {
    let name = &task.task.metadata.name;
    name.contains("-web-platform-tests-") || name.starts_with("spidermonkey")
}

struct TaskclusterUrls {
    index_base: String,
    queue_base: String,
}

fn get_taskcluster_urls(taskcluster_base: &str) -> TaskclusterUrls {
    if taskcluster_base == "https://taskcluster.net" {
        TaskclusterUrls {
            index_base: "https://index.taskcluster.net/v1/".into(),
            queue_base: "https://queue.taskcluster.net/v1/".into(),
        }
    } else {
        TaskclusterUrls {
            index_base: format!("{}/api/index/v1/", taskcluster_base),
            queue_base: format!("{}/api/queue/v1/", taskcluster_base),
        }
    }
}

fn wpt_complete<'a, I>(mut tasks: I) -> bool
where
    I: Iterator<Item = &'a TaskGroupTask>,
{
    tasks.all(|task| task.status.state.is_complete())
}

fn run() -> Result<()> {
    let matches = parse_args().get_matches();
    let branch = matches.value_of("branch").unwrap();
    let commit = matches.value_of("commit").unwrap();
    let taskcluster_base = matches.value_of("taskcluster_url").unwrap();
    let log_format = value_t_or_exit!(matches, "log_format", LogFormat);
    let taskcluster_urls = get_taskcluster_urls(taskcluster_base);

    if !commit_is_valid(&commit) {
        return Err(Error::String(
            "Commit `{}` needs to be between 12 and 40 characters in length".into()))
    }

    let cur_dir = env::current_dir().expect("Invalid working directory");
    let out_dir: PathBuf = if let Some(dir) = matches.value_of("out_dir") {
        cur_dir.join(dir)
    } else {
        cur_dir
    };
    if !out_dir.is_dir() {
        return Err(Error::String(format!("{} is not a directory", out_dir.display())))
    }

    let client = reqwest::Client::new();

    let commit = get_revision(&client, &branch, &commit)?.node;

    let taskgroup = get_taskgroup(&client, &taskcluster_urls, &branch, &commit)?;

    let tasks = get_taskgroup_tasks(&client, &taskcluster_urls, &taskgroup.taskId)?;
    let wpt_tasks: Vec<TaskGroupTask> = tasks.tasks.into_iter().filter(|task| is_wpt_task(task)).collect();

    if matches.is_present("check_complete") {
        if !wpt_complete(wpt_tasks.iter()) {
            return Err(Error::String("wpt tasks are not yet complete".into()))
        }
    }

    fetch_job_logs(&client, &taskcluster_urls, &out_dir, wpt_tasks, log_format);
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => {},
        Err(e) => {
            println!("{:?}", e);
            process::exit(1);
        }
    }
}
