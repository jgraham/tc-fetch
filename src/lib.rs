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
) -> Vec<PathBuf> {
    let mut pool = scoped_threadpool::Pool::new(8);
    let paths = Arc::new(Mutex::new(Vec::with_capacity(tasks.len())));

    pool.scoped(|scope| {
        for task in tasks.iter() {
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
                println!("{} {:?}", task_id, artifacts);
                // TODO: this selects too many artifacts, should split on separator and check for an exact match
                let artifact = artifacts
                    .iter()
                    .find(|&artifact| artifact.name.ends_with(artifact_name));
                if let Some(artifact) = artifact {
                    let name = PathBuf::from(format!(
                        "{}-{}-{}",
                        task.task.metadata.name.replace('/', "-"),
                        &task.status.taskId,
                        artifact_name
                    ));
                    let dest = out_dir.join(name);

                    if dest.exists() {
                        println!("{} exists locally, skipping", dest.to_string_lossy());
                    } else {
                        let log_url = taskcluster.get_log_url(&task_id, artifact);
                        println!("Downloading {} to {}", log_url, dest.to_string_lossy());
                        download(&client, &dest, &log_url);
                    }
                    {
                        let mut paths = paths.lock().unwrap();
                        (*paths).push(dest);
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
        println!("filter match {} {} {}", name, self.filter_re, is_match);
        is_match
    }
}

pub fn download_artifacts(
    taskcluster_base: Option<&str>,
    repo: &str,
    commit: &str,
    task_filters: Option<Vec<TaskFilter>>,
    artifact_name: Option<&str>,
    check_complete: bool,
    out_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let client = reqwest::blocking::Client::new();

    let (taskgroup, task_filters, default_artifact_name, taskcluster) = if repo == "wpt" {
        let ci = ghwpt::GithubCI::new(taskcluster_base);
        let task_filters = task_filters.unwrap_or_else(ghwpt::GithubCI::default_task_filter);
        let taskgroup = ci.get_taskgroup(&client, commit)?;
        (
            taskgroup,
            task_filters,
            ghwpt::GithubCI::default_artifact_name(),
            ci.into_taskcluster(),
        )
    } else {
        let ci = hgmo::HgmoCI::for_repo(taskcluster_base, repo.into())
            .ok_or_else(|| Error::String(format!("No such repository {}", repo)))?;
        let task_filters = task_filters.unwrap_or_else(hgmo::HgmoCI::default_task_filter);
        let taskgroup = ci.get_taskgroup(&client, commit)?;
        (
            taskgroup,
            task_filters,
            hgmo::HgmoCI::default_artifact_name(),
            ci.into_taskcluster(),
        )
    };

    let tasks = taskcluster.get_taskgroup_tasks(&client, &taskgroup)?;
    let tasks: Vec<TaskGroupTask> = tasks
        .into_iter()
        .filter(|task| include_task(task, &task_filters))
        .collect();

    if check_complete && !tasks_complete(tasks.iter()) {
        return Err(Error::String("wpt tasks are not yet complete".into()));
    }

    Ok(fetch_job_logs(
        &client,
        &taskcluster,
        out_dir,
        tasks,
        artifact_name.unwrap_or(default_artifact_name),
    ))
}
