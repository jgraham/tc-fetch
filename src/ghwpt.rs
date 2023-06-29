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

    fn get_taskgroups(
        &self,
        client: &reqwest::blocking::Client,
        commit: &str,
    ) -> Result<Vec<String>> {
        let check_runs = gh::get_checks(client, "web-platform-tests", "wpt", commit)?;
        let mut task_names = vec![];
        for check in check_runs.iter() {
            if check.name == "wpt-decision-task" {
                if let Some(ref details_url) = check.details_url {
                    if let Some(task_name) = details_url.rsplit('/').next().map(|x| x.to_string()) {
                        task_names.push(task_name.into());
                    }
                } else {
                    return Err(Error::String(
                        "No details_url for wpt-decision-task check; can't find taskgroup".into(),
                    ));
                }
            }
        }
        if task_names.is_empty() {
            return Err(Error::String("Unable to find decision task".into()));
        }
        Ok(task_names)
    }
}
