use crate::utils::{get_json, url};
use crate::Result;
use reqwest;
use serde_derive::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct ChecksResponse {
    pub total_count: u64,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRun {
    pub id: u64,
    pub head_sha: String,
    pub node_id: String,
    pub external_id: Option<String>,
    pub url: String,
    pub html_url: Option<String>,
    pub details_url: Option<String>,
    pub status: RunStatus,
    pub conclusion: Option<RunConclusion>, //TODO: enum,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub output: CheckOutput,
    pub name: String,
    pub check_suite: Option<CheckSuite>,
    pub app: Option<GithubApp>,
    pub pull_requests: Vec<PullRequestMinimal>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunConclusion {
    Success,
    Failure,
    Neutral,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
}

#[derive(Debug, Deserialize)]
pub struct CheckOutput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub text: Option<String>,
    pub annotations_count: u64,
    pub annotations_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckSuite {
    pub id: u64,
}

#[derive(Debug, Deserialize)]
pub struct GithubApp {
    pub id: u64,
    pub slug: String,
    pub node_id: String,
    pub owner: Option<SimpleUser>,
    pub name: String,
    pub description: Option<String>,
    pub external_url: String,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    pub permissions: BTreeMap<String, String>,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SimpleUser {
    pub login: String,
    pub id: u64,
    pub node_id: String,
    pub url: String,
    pub repos_url: String,
    pub events_url: String,
    pub avatar_url: String,
    pub gravatar_id: Option<String>,
    pub html_url: String,
    pub followers_url: String,
    pub following_url: String,
    pub gists_url: String,
    pub starred_url: String,
    pub subscriptions_url: String,
    pub organizations_url: String,
    pub received_events_url: String,
    #[serde(rename = "type")]
    pub user_type: String,
    pub site_admin: bool,
    pub starred_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Permissions {
    pub metadata: String,    // TODO: enum
    pub contents: String,    // TODO: enum
    pub issues: String,      // TODO: enum
    pub single_file: String, // TODO: enum
}

#[derive(Debug, Deserialize)]
pub struct PullRequestMinimal {
    pub url: String,
    pub id: u64,
    pub number: u64,
    pub head: Ref,
}

#[derive(Debug, Deserialize)]
pub struct Ref {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
    pub repo: Repo,
}

#[derive(Debug, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub url: String,
    pub name: String,
}

pub fn get_checks(
    client: &reqwest::blocking::Client,
    owner: &str,
    repo: &str,
    sha1: &str,
) -> Result<Vec<CheckRun>> {
    let url_suffix = format!("repos/{}/{}/commits/{}/check-runs", owner, repo, sha1);
    let mut page = 0;
    let mut checks = Vec::new();
    let mut checks_total: Option<u64> = None;
    while checks_total.is_none() || checks_total != Some(checks.len() as u64) {
        page += 1;
        let base_url = &url("https://api.github.com/", &url_suffix);
        let mut query = vec![
            ("per_page".into(), "100".into()),
            ("filter".into(), "all".into()),
        ];
        if page > 1 {
            query.push(("page".into(), page.to_string()));
        }
        let checks_resp: ChecksResponse = get_json(
            client,
            base_url,
            Some(query),
            Some(vec![
                ("user-agent".to_string(), "tcfetch/0.4".to_string()),
                (
                    "Accept".to_string(),
                    "application/vnd.github+json".to_string(),
                ),
                ("X-GitHub-Api-Version".to_string(), "2022-11-28".to_string()),
            ]),
        )?;
        checks_total = Some(checks_resp.total_count);
        checks.extend(checks_resp.check_runs.into_iter())
    }
    Ok(checks)
}
