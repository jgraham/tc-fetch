use crate::utils::{get_json, url};
use crate::{Result, TaskFilter};
use reqwest;
use serde_derive::Deserialize;
use std::collections::BTreeMap;

pub(crate) trait TaskclusterCI {
    fn default_artifact_name(&self) -> &'static str;
    fn default_task_filter(&self) -> Vec<TaskFilter>;
    fn get_taskgroups(
        &self,
        client: &reqwest::blocking::Client,
        commit: &str,
    ) -> Result<Vec<String>>;
    fn taskcluster(&self) -> &Taskcluster;
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Unscheduled,
    Pending,
    Running,
    Completed,
    Failed,
    Exception,
}

impl TaskState {
    pub fn is_complete(&self) -> bool {
        match self {
            TaskState::Unscheduled | TaskState::Pending | TaskState::Running => false,
            TaskState::Completed | TaskState::Failed | TaskState::Exception => true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct IndexResponse {
    pub namespace: String,
    pub taskId: String,
    pub rank: u64,
    pub expires: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct ArtifactsResponse {
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Artifact {
    pub storageType: String,
    pub name: String,
    pub expires: String,
    pub contentType: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TaskGroupResponse {
    pub taskGroupId: String,
    pub tasks: Vec<TaskGroupTask>,
    pub continuationToken: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TaskGroupTask {
    pub status: TaskGroupTaskStatus,
    pub task: Task,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TaskGroupTaskStatus {
    pub taskId: String,
    pub provisionerId: String,
    pub workerType: String,
    pub schedulerId: String,
    pub taskGroupId: String,
    pub deadline: String, // Should be a time type
    pub expires: String,  // Should be a time type
    pub retriesLeft: u64,
    pub state: TaskState,
    pub runs: Vec<TaskRun>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TaskRun {
    pub runId: u64,
    pub state: TaskState,
    pub reasonCreated: String,          // Should be an enum
    pub reasonResolved: Option<String>, // Should be an enum
    pub workerGroup: Option<String>,
    pub workerId: Option<String>,
    pub takenUntil: Option<String>, // Should be a time type
    pub scheduled: Option<String>,  // Should be a time type
    pub started: Option<String>,    // Should be a time type
    pub resolved: Option<String>,   // Should be a time type
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Task {
    pub provisionerId: String,
    pub workerType: String,
    pub schedulerId: String,
    pub taskGroupId: String,
    pub metadata: TaskMetadata,
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TaskMetadata {
    pub owner: String,
    pub source: String,
    pub description: String,
    pub name: String,
}

pub struct Taskcluster {
    pub index_base: String,
    pub queue_base: String,
}

impl Taskcluster {
    pub fn new(taskcluster_base: &str) -> Taskcluster {
        if taskcluster_base == "https://taskcluster.net" {
            Taskcluster {
                index_base: "https://index.taskcluster.net/v1/".into(),
                queue_base: "https://queue.taskcluster.net/v1/".into(),
            }
        } else {
            Taskcluster {
                index_base: format!("{}/api/index/v1/", taskcluster_base),
                queue_base: format!("{}/api/queue/v1/", taskcluster_base),
            }
        }
    }

    pub fn get_taskgroup_tasks(
        &self,
        client: &reqwest::blocking::Client,
        taskgroup_id: &str,
    ) -> Result<Vec<TaskGroupTask>> {
        let url_suffix = format!("task-group/{}/list", taskgroup_id);
        let mut tasks = Vec::new();
        let mut continuation_token: Option<String> = None;
        loop {
            let query = continuation_token.map(|token| vec![("continuationToken".into(), token)]);
            let data: TaskGroupResponse =
                get_json(client, &url(&self.queue_base, &url_suffix), query, None)?;
            tasks.extend(data.tasks);
            if data.continuationToken.is_none() {
                break;
            }
            continuation_token = data.continuationToken;
        }
        Ok(tasks)
    }

    pub fn get_artifacts(
        &self,
        client: &reqwest::blocking::Client,
        task_id: &str,
    ) -> Result<Vec<Artifact>> {
        let url_suffix = format!("task/{}/artifacts", task_id);
        let artifacts: ArtifactsResponse =
            get_json(client, &url(&self.queue_base, &url_suffix), None, None)?;
        Ok(artifacts.artifacts)
    }

    pub fn get_log_url(&self, task_id: &str, artifact: &Artifact) -> String {
        let task_url = format!("task/{}/artifacts", task_id);
        url(
            &self.queue_base,
            &format!("{}/{}", &task_url, artifact.name),
        )
    }
}

pub fn tasks_complete<'a, I>(mut tasks: I) -> bool
where
    I: Iterator<Item = &'a TaskGroupTask>,
{
    tasks.all(|task| task.status.state.is_complete())
}
