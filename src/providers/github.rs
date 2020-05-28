use std::{collections::HashSet, convert::From, fmt, time::Duration};

use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::error::Error as JsonError;

const API_BASE_URL: &str = "https://api.github.com";
const PER_PAGE: usize = 100;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    reason: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<JsonError> for Error {
    fn from(err: JsonError) -> Self {
        Error {
            reason: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error {
            reason: err.to_string(),
        }
    }
}

pub struct GitHub {
    token: String,
    client: reqwest::Client,
    deliver_labels: HashSet<String>,
    deliever_after: Duration,
}

struct Header {
    key: String,
    value: String,
}

struct Repo {
    owner: String,
    repo: String,
}

impl From<String> for Repo {
    fn from(r: String) -> Self {
        let parsed = r.split("/").map(Into::into).collect::<Vec<String>>();
        Repo {
            owner: parsed[0].to_owned(),
            repo: parsed[1].to_owned(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct User {
    login: String,
}

#[derive(Serialize, Deserialize)]
pub struct Pull {
    html_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Assignee {
    id: i64,
    login: String,
}

#[derive(Serialize, Deserialize)]
pub struct Label {
    id: i64,
    name: String,
    description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Issue {
    number: i32,
    title: String,
    assignee: Option<Assignee>,
    #[serde(skip_deserializing)]
    owner: String,
    #[serde(skip_deserializing)]
    repo: String,
    pull_request: Option<Pull>,
    created_at: DateTime<Utc>,
    author_association: String,
    labels: Vec<Label>,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "https://github.com/{}/{}/issues/{}",
            self.owner, self.repo, self.number
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct Comment {
    html_url: String,
    author_association: String,
}

impl GitHub {
    pub fn new(token: String, deliver_labels: Vec<String>, deliever_after: Duration) -> Self {
        let mut auth_header = "token ".to_owned();
        auth_header.push_str(&token);
        GitHub {
            token: auth_header,
            client: reqwest::Client::new(),
            deliver_labels: deliver_labels
                .into_iter()
                .map(|label| label.to_lowercase())
                .collect(),
            deliever_after: deliever_after,
        }
    }

    async fn request(&self, url: &str, headers: Vec<Header>) -> Result<String> {
        let mut req = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, "pingbot")
            .header(reqwest::header::AUTHORIZATION, &self.token[..]);
        for header in headers {
            req = req.header(&header.key[..], &header.value[..]);
        }
        let res = req.send().await?.text().await?;
        Ok(res)
    }

    pub async fn get_user_result(&self) -> Result<String> {
        let url = format!("{}/user", API_BASE_URL);
        let res = self.request(&url[..], vec![]).await?;
        let u: User = serde_json::from_str(&res[..])?;
        Ok(u.login.to_owned())
    }

    pub async fn get_opened_issues(&self, raw: Vec<String>) -> Result<Vec<Issue>> {
        let now = Utc::now();
        let repos = parse_repos(raw);
        let mut opened_all = vec![];
        for repo in repos {
            println!("process {}/{}", repo.owner, repo.repo);
            let issues = self.get_opened_issues_by_repo(&repo).await?;
            opened_all.extend(issues);
        }

        println!("all issues {}", opened_all.len());

        let opened_issues: Vec<Issue> = opened_all
            .into_iter()
            .filter(|issue| {
                if !self.if_deliver_by_label(&issue) {
                    return false;
                }
                if now.signed_duration_since(issue.created_at).num_seconds()
                    < self.deliever_after.as_secs() as i64
                {
                    return false;
                }
                issue.pull_request.is_none() // && issue.assignee.is_none() && !if_member(&issue.author_association)
            })
            .collect();

        println!("opened issues {}", opened_issues.len());

        let mut no_comment_issue = Vec::<Issue>::new();
        for issue in opened_issues {
            let comment_num = self.get_comments_by_issue(&issue).await?;
            if comment_num == 0 {
                no_comment_issue.push(issue);
            }
        }
        println!("no comment issues {}", no_comment_issue.len());

        Ok(no_comment_issue)
    }

    async fn get_opened_issues_by_repo(&self, repo: &Repo) -> Result<Vec<Issue>> {
        let mut all = Vec::<Issue>::new();
        let mut page = 0;

        while all.len() == page * PER_PAGE {
            page += 1;
            let url = format!(
                "{}/repos/{}/{}/issues?page={}&per_page={}",
                API_BASE_URL, repo.owner, repo.repo, page, PER_PAGE
            );
            let headers = vec![Header {
                key: "Accept".to_owned(),
                value: "application/vnd.github.machine-man-preview".to_owned(),
            }];
            let res = self.request(&url[..], headers).await?;
            let batch: Vec<Issue> = serde_json::from_str(&res[..])?;
            all.extend(batch);
        }
        println!("all ok");

        Ok(all
            .into_iter()
            .map(|mut issue| {
                issue.owner = repo.owner.to_owned();
                issue.repo = repo.repo.to_owned();
                issue
            })
            .collect())
    }

    async fn get_comments_by_issue(&self, issue: &Issue) -> Result<usize> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}/comments?per_page={}",
            API_BASE_URL, issue.owner, issue.repo, issue.number, PER_PAGE
        );
        let res = self.request(&url[..], vec![]).await?;
        let comments: Vec<Comment> = serde_json::from_str(&res[..])?;
        let member_comments: Vec<Comment> = comments
            .into_iter()
            .filter(|comment| if_member(&comment.author_association))
            .collect();
        Ok(member_comments.len())
    }

    fn if_deliver_by_label(&self, issue: &Issue) -> bool {
        for label in &issue.labels {
            let lower_label = label.name.to_lowercase();
            if self.deliver_labels.contains(&lower_label) {
                return true;
            }
        }
        false
    }
}

fn parse_repos(raw: Vec<String>) -> Vec<Repo> {
    raw.into_iter().map(Into::into).collect()
}

fn if_member(relation: &String) -> bool {
    relation == "OWNER"
        || relation == "COLLABORATOR"
        || relation == "MEMBER"
        || relation == "CONTRIBUTOR"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_client() -> GitHub {
        let deliver_labels = vec!["l1".to_owned(), "l2".to_owned()];
        GitHub::new(
            "".to_owned(),
            deliver_labels,
            Duration::new(12 * 60 * 60, 0),
        )
    }

    fn new_issue_with_labels(labels: Vec<String>) -> Issue {
        Issue {
            number: 0,
            title: "title".to_owned(),
            assignee: None,
            owner: "".to_owned(),
            repo: "".to_owned(),
            pull_request: None,
            created_at: Utc::now(),
            author_association: "".to_owned(),
            labels: labels
                .into_iter()
                .map(|name| Label {
                    id: 0,
                    name: name,
                    description: Some("".to_owned()),
                })
                .collect(),
        }
    }

    #[test]
    fn deliver_label() {
        let client = new_client();
        let issue1 = new_issue_with_labels(vec!["l1".to_owned(), "l2".to_owned()]);
        let issue2 = new_issue_with_labels(vec!["l2".to_owned(), "l3".to_owned()]);
        let issue3 = new_issue_with_labels(vec!["l3".to_owned(), "l4".to_owned()]);
        assert_eq!(client.if_deliver_by_label(&issue1), true);
        assert_eq!(client.if_deliver_by_label(&issue2), true);
        assert_eq!(client.if_deliver_by_label(&issue3), false);
    }
}
