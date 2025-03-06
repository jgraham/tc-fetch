pub mod gh;
mod ghwpt;
mod hgmo;
pub mod taskcluster;
mod utils;

use regex::Regex;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use taskcluster::{tasks_complete, TaskGroupTask, Taskcluster, TaskclusterCI};
use thiserror::Error;
use utils::download;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0}")]
    String(String),
}

fn fetch_job_logs(
    client: &reqwest::blocking::Client,
    taskcluster: &Taskcluster,
    out_dir: &Path,
    tasks: Vec<TaskGroupTask>,
    artifact_name: &str,
    compress: bool,
) -> Vec<(TaskGroupTask, PathBuf)> {
    let mut pool = scoped_threadpool::Pool::new(8);
    let paths = Arc::new(Mutex::new(Vec::with_capacity(tasks.len())));

    // TODO: Convert this to async
    pool.scoped(|scope| {
        for task in tasks.into_iter() {
            scope.execute(|| {
                let task_id = task.status.taskId.clone();
                let client = client.clone();

                let artifacts = match taskcluster.get_artifacts(&client, &task_id) {
                    Ok(x) => x,
                    Err(err) => {
                        eprintln!("{}", err);
                        return;
                    }
                };
                // TODO: this selects too many artifacts, should split on separator and check for an exact match
                let artifact = artifacts
                    .iter()
                    .find(|&artifact| artifact.name.ends_with(artifact_name));
                if let Some(artifact) = artifact {
                    let ext = if compress { ".zstd" } else { "" };

                    let name = PathBuf::from(format!(
                        "{}-{}-{}{}",
                        task.task.metadata.name.replace('/', "-"),
                        &task.status.taskId,
                        artifact_name,
                        ext
                    ));
                    let dest = out_dir.join(name);

                    if dest.exists() {
                        println!("{} exists locally, skipping", dest.to_string_lossy());
                    } else {
                        let log_url = taskcluster.get_log_url(&task_id, artifact);

                        println!("Downloading {} to {}", log_url, dest.to_string_lossy());
                        download(&client, &dest, &log_url, compress);
                    }
                    {
                        let mut paths = paths.lock().unwrap();
                        (*paths).push((task, dest));
                    }
                }
            });
        }
    });
    Arc::try_unwrap(paths).unwrap().into_inner().unwrap()
}

fn include_task(task: &TaskGroupTask, task_filters: &[TaskFilter]) -> bool {
    let name = &task.task.metadata.name;
    task_filters.iter().all(|filter| filter.is_match(name))
}

#[derive(Debug)]
pub struct TaskFilter {
    filter_re: Regex,
    invert: bool,
}

impl TaskFilter {
    pub fn new(filter_str: &str) -> Result<TaskFilter> {
        let invert = filter_str.starts_with('!');
        let mut re_str = if invert { &filter_str[1..] } else { filter_str };
        let filter_string: String;
        if !re_str.starts_with('^') {
            filter_string = format!("^.*(?:{})", re_str);
            re_str = &filter_string
        }
        Regex::new(re_str)
            .map(|filter_re| TaskFilter { filter_re, invert })
            .map_err(|_| {
                Error::String(format!(
                    "Filter `{}` can't be parsed as a regular expression",
                    filter_str
                ))
            })
    }

    pub(crate) fn is_match(&self, name: &str) -> bool {
        let mut is_match = self.filter_re.is_match(name);
        if self.invert {
            is_match = !is_match;
        }
        is_match
    }
}

fn get_ci(repo: &str, taskcluster_base: Option<&str>) -> Option<Box<dyn TaskclusterCI>> {
    match repo {
        "wpt" => Some(Box::new(ghwpt::GithubCI::new(taskcluster_base))),
        _ => {
            if let Some(ci) = hgmo::HgmoCI::for_repo(taskcluster_base, repo.into()) {
                Some(Box::new(ci))
            } else {
                None
            }
        }
    }
}

pub fn check_complete(taskcluster_base: Option<&str>, repo: &str, commit: &str) -> Result<bool> {
    let client = reqwest::blocking::Client::new();
    let ci = get_ci(repo, taskcluster_base)
        .ok_or_else(|| Error::String(format!("No such repo {}", repo)))?;
    let taskgroups = ci.get_taskgroups(&client, commit)?;
    let mut tasks = Vec::new();
    for taskgroup in taskgroups {
        tasks.extend(ci.taskcluster().get_taskgroup_tasks(&client, &taskgroup)?)
    }
    Ok(tasks_complete(tasks.iter()))
}

pub fn download_artifacts(
    taskcluster_base: Option<&str>,
    repo: &str,
    commit: &str,
    task_filters: Option<Vec<TaskFilter>>,
    artifact_name: Option<&str>,
    check_complete: bool,
    out_dir: &Path,
    compress: bool,
) -> Result<Vec<(TaskGroupTask, PathBuf)>> {
    let client = reqwest::blocking::Client::new();

    let ci = get_ci(repo, taskcluster_base)
        .ok_or_else(|| Error::String(format!("No such repo {}", repo)))?;

    let task_filters = task_filters.unwrap_or_else(|| ci.default_task_filter());
    let taskgroups = ci.get_taskgroups(&client, commit)?;
    let artifact_name = artifact_name.unwrap_or_else(|| ci.default_artifact_name());

    let mut tasks = Vec::new();
    for taskgroup in taskgroups {
        tasks.extend(ci.taskcluster().get_taskgroup_tasks(&client, &taskgroup)?)
    }
    let tasks: Vec<TaskGroupTask> = tasks
        .into_iter()
        .filter(|task| include_task(task, &task_filters))
        .collect();

    if check_complete && !tasks_complete(tasks.iter()) {
        return Err(Error::String("wpt tasks are not yet complete".into()));
    }

    Ok(fetch_job_logs(
        &client,
        ci.taskcluster(),
        out_dir,
        tasks,
        artifact_name,
        compress,
    ))
}
