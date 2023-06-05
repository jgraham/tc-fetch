use crate::taskcluster::{Taskcluster, TaskclusterCI};
use crate::{gh, TaskFilter};
use crate::{Error, Result};

pub(crate) struct GithubCI {
    taskcluster: Taskcluster,
}

impl GithubCI {
    pub(crate) fn new(taskcluster_base: Option<&str>) -> Self {
        GithubCI {
            taskcluster: Taskcluster::new(
                taskcluster_base.unwrap_or("https://community-tc.services.mozilla.com"),
            ),
        }
    }
}

impl TaskclusterCI for GithubCI {
    fn taskcluster(&self) -> &Taskcluster {
        &self.taskcluster
    }

    fn default_task_filter(&self) -> Vec<TaskFilter> {
        vec![TaskFilter::new("-chrome-|-firefox-").expect("Invalid default task filter")]
    }

    fn default_artifact_name(&self) -> &'static str {
        "wpt_report.json.gz"
    }

    fn get_taskgroup(&self, client: &reqwest::blocking::Client, commit: &str) -> Result<String> {
        let check_runs = gh::get_checks(client, "web-platform-tests", "wpt", commit)?;
        let mut task_name = None;
        for check in check_runs.iter() {
            if check.name == "wpt-decision-task" {
                if let Some(ref details_url) = check.details_url {
                    task_name = details_url.rsplit('/').next().map(|x| x.to_string());
                    break;
                } else {
                    return Err(Error::String(
                        "No details_url for wpt-decision-task check; can't find taskgroup".into(),
                    ));
                }
            }
        }
        task_name.ok_or_else(|| Error::String("Unable to find decision task".into()))
    }
}
