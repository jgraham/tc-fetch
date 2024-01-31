use crate::taskcluster::{IndexResponse, Taskcluster, TaskclusterCI};
use crate::utils::{get_json, url};
use crate::{Error, Result, TaskFilter};
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Revision {
    pub node: String,
    pub desc: String,
    pub user: String,
    pub parents: Vec<String>,
    pub phase: String,
    pub pushid: u64,
    pub pushuser: String,
}

fn hg_path(repo: &str) -> Option<&'static str> {
    match repo {
        "try" => Some("try"),
        "mozilla-release" => Some("releases/mozilla-release"),
        "mozilla-beta" => Some("releases/mozilla-beta"),
        "mozilla-central" => Some("mozilla-central"),
        "mozilla-inbound" => Some("integration/mozilla-inbound"),
        "autoland" => Some("integration/autoland"),
        _ => None,
    }
}

pub(crate) struct HgmoCI {
    taskcluster: Taskcluster,
    repo: String,
    hg_path: &'static str,
}

impl HgmoCI {
    pub(crate) fn for_repo(taskcluster_base: Option<&str>, repo: String) -> Option<Self> {
        hg_path(&repo).map(|hg_path| HgmoCI {
            taskcluster: Taskcluster::new(
                taskcluster_base.unwrap_or("https://firefox-ci-tc.services.mozilla.com"),
            ),
            repo,
            hg_path,
        })
    }

    fn expand_revision(
        &self,
        client: &reqwest::blocking::Client,
        commit: &str,
    ) -> Result<Option<String>> {
        let url_ = format!(
            "https://hg.mozilla.org/{}/json-rev/{}",
            self.hg_path, commit
        );

        let resp =
            get_json::<Revision>(client, &url_, None, None).map(|revision| Some(revision.node));
        if let Err(Error::Reqwest(ref err)) = resp {
            if let Some(status_code) = err.status() {
                if status_code == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
            }
        }
        resp
    }
}

fn commit_is_valid(commit: &str) -> bool {
    commit.len() >= 12 && commit.len() <= 40
}

impl TaskclusterCI for HgmoCI {
    fn taskcluster(&self) -> &Taskcluster {
        &self.taskcluster
    }

    fn default_task_filter(&self) -> Vec<TaskFilter> {
        vec![TaskFilter::new("-web-platform-tests-|-spidermonkey-")
            .expect("Invalid default task filter")]
    }

    fn default_artifact_name(&self) -> &'static str {
        "wptreport.json"
    }

    fn get_taskgroups(
        &self,
        client: &reqwest::blocking::Client,
        commit: &str,
    ) -> Result<Vec<String>> {
        if !commit_is_valid(commit) {
            return Err(Error::String(format!(
                "Commit `{}` needs to be between 12 and 40 characters in length",
                commit
            )));
        }

        let commit = self
            .expand_revision(client, commit)?
            .ok_or_else(|| Error::String(format!("No such revision {}", commit)))?;

        let index = format!(
            "gecko.v2.{}.revision.{}.taskgraph.decision",
            self.repo, commit
        );
        Ok(vec![
            get_json::<IndexResponse>(
                client,
                &url(&self.taskcluster.index_base, &format!("task/{}", index)),
                None,
                None,
            )?
            .taskId,
        ])
    }
}
