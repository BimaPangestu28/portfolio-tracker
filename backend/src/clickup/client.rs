use async_trait::async_trait;

/// A ClickUp List, surfaced to the assistant as a "project".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// Fields for creating a task.
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub name: String,
    pub due_date_ms: Option<i64>,
    /// Sets the ClickUp `Billable` checkbox custom field when present.
    pub billable: Option<bool>,
    /// Sets the ClickUp `Amount` money custom field (IDR) when present.
    pub amount: Option<f64>,
}

/// A ClickUp task as read back from the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub status: String,
    pub due_date_ms: Option<i64>,
}

/// A completed/closed ClickUp time entry, used for reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeEntry {
    pub task_id: String,
    pub task_name: String,
    pub project_name: String,
    pub duration_ms: i64,
    pub start_ms: i64,
    pub billable: bool,
}

/// The currently running timer, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningEntry {
    pub task_name: String,
    pub started_ms: i64,
}

#[derive(Debug)]
pub enum ClickUpError {
    NoToken,
    Http(String),
    Api { status: u16, body: String },
}

impl std::fmt::Display for ClickUpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickUpError::NoToken => write!(f, "CLICKUP_API_TOKEN tidak diset"),
            ClickUpError::Http(e) => write!(f, "gangguan jaringan ClickUp: {e}"),
            ClickUpError::Api { status, body } => write!(f, "ClickUp error {status}: {body}"),
        }
    }
}
impl std::error::Error for ClickUpError {}

/// The seam the assistant tools depend on. A fake implements this in tests;
/// `ClickUpClient` implements it against the real API.
#[async_trait]
pub trait ClickUpApi: Send + Sync {
    /// Lists in the configured Space (= projects).
    async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError>;
    /// Create a List in the configured Space; returns the new project.
    async fn create_project(&self, name: &str) -> Result<Project, ClickUpError>;
    /// Create a task in the given List; returns the new task id.
    async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError>;
    /// Open tasks in a List.
    async fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>, ClickUpError>;
    /// Mark a task complete (sets its status to the configured done status).
    async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError>;
}

/// Real reqwest-backed client. Reads token + space id from env.
pub struct ClickUpClient {
    http: reqwest::Client,
    token: String,
    space_id: String,
    done_status: String,
    billable_field: String,
    amount_field: String,
}

impl ClickUpClient {
    /// Build from env: `CLICKUP_API_TOKEN` (required), `CLICKUP_SPACE_ID`
    /// (required); `CLICKUP_DONE_STATUS`, `CLICKUP_BILLABLE_FIELD`,
    /// `CLICKUP_AMOUNT_FIELD` (optional, defaulted). The v2 endpoints used here
    /// are space-scoped, so no workspace/team id is needed.
    pub fn from_env() -> Result<Self, ClickUpError> {
        let token = std::env::var("CLICKUP_API_TOKEN").map_err(|_| ClickUpError::NoToken)?;
        if token.trim().is_empty() {
            return Err(ClickUpError::NoToken);
        }
        let space_id = std::env::var("CLICKUP_SPACE_ID")
            .map_err(|_| ClickUpError::Api { status: 0, body: "CLICKUP_SPACE_ID tidak diset".into() })?;
        let done_status = std::env::var("CLICKUP_DONE_STATUS").unwrap_or_else(|_| "complete".into());
        let billable_field = std::env::var("CLICKUP_BILLABLE_FIELD").unwrap_or_else(|_| "Billable".into());
        let amount_field = std::env::var("CLICKUP_AMOUNT_FIELD").unwrap_or_else(|_| "Amount".into());
        Ok(Self { http: reqwest::Client::new(), token, space_id, done_status, billable_field, amount_field })
    }

    fn classify(status: reqwest::StatusCode, body: String) -> ClickUpError {
        ClickUpError::Api { status: status.as_u16(), body }
    }

    /// (id, name) of every custom field visible on a list.
    async fn list_fields(&self, list_id: &str) -> Result<Vec<(String, String)>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/field");
        let resp = self.http.get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        let fields = parsed["fields"].as_array().map(|arr| {
            arr.iter().filter_map(|f| {
                Some((f["id"].as_str()?.to_string(), f["name"].as_str()?.to_string()))
            }).collect()
        }).unwrap_or_default();
        Ok(fields)
    }
}

#[async_trait]
impl ClickUpApi for ClickUpClient {
    async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/space/{}/list?archived=false", self.space_id);
        let resp = self.http.get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        let projects = parsed["lists"].as_array().map(|arr| {
            arr.iter().filter_map(|l| {
                Some(Project {
                    id: l["id"].as_str()?.to_string(),
                    name: l["name"].as_str()?.to_string(),
                })
            }).collect()
        }).unwrap_or_default();
        Ok(projects)
    }

    async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/space/{}/list", self.space_id);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "name": name }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(Project {
            id: parsed["id"].as_str().unwrap_or_default().to_string(),
            name: parsed["name"].as_str().unwrap_or(name).to_string(),
        })
    }

    async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/task");
        let mut payload = serde_json::json!({ "name": task.name });
        if let Some(ms) = task.due_date_ms {
            payload["due_date"] = serde_json::json!(ms);
        }
        if task.billable.is_some() || task.amount.is_some() {
            let fields = self.list_fields(list_id).await?;
            let mut custom = Vec::new();
            if let Some(b) = task.billable {
                if let Some((id, _)) = fields.iter().find(|(_, n)| n.eq_ignore_ascii_case(&self.billable_field)) {
                    custom.push(serde_json::json!({ "id": id, "value": b }));
                }
            }
            if let Some(a) = task.amount {
                if let Some((id, _)) = fields.iter().find(|(_, n)| n.eq_ignore_ascii_case(&self.amount_field)) {
                    custom.push(serde_json::json!({ "id": id, "value": a }));
                }
            }
            if !custom.is_empty() {
                payload["custom_fields"] = serde_json::json!(custom);
            }
        }
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&payload)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(parsed["id"].as_str().unwrap_or_default().to_string())
    }

    async fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>, ClickUpError> {
        // No include_closed=true, so ClickUp returns only open tasks — that's
        // what makes a completed task disappear from list_tasks/the briefing.
        // (Also no page= param: results are capped at ClickUp's first page.)
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/task?archived=false");
        let resp = self.http.get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        let tasks = parsed["tasks"].as_array().map(|arr| {
            arr.iter().filter_map(|t| {
                Some(Task {
                    id: t["id"].as_str()?.to_string(),
                    name: t["name"].as_str()?.to_string(),
                    status: t["status"]["status"].as_str().unwrap_or("").to_string(),
                    due_date_ms: t["due_date"].as_str().and_then(|s| s.parse::<i64>().ok()),
                })
            }).collect()
        }).unwrap_or_default();
        Ok(tasks)
    }

    async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/task/{task_id}");
        let resp = self.http.put(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "status": self.done_status }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_errors_without_token() {
        let prev = std::env::var("CLICKUP_API_TOKEN").ok();
        std::env::remove_var("CLICKUP_API_TOKEN");
        let result = ClickUpClient::from_env();
        if let Some(v) = prev { std::env::set_var("CLICKUP_API_TOKEN", v); }
        assert!(result.is_err(), "missing token must be an error");
    }
}
