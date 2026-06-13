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
    /// Start a timer on a task.
    async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError>;
    /// Stop the running timer; `Ok(None)` if nothing was running.
    async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError>;
    /// The currently running timer, if any.
    async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError>;
    /// Completed time entries overlapping [start_ms, end_ms].
    async fn time_entries(&self, start_ms: i64, end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError>;
    /// Log a manual time entry on a task.
    async fn add_time_entry(&self, task_id: &str, duration_ms: i64, start_ms: i64) -> Result<(), ClickUpError>;
}

/// Real reqwest-backed client. Reads token + space id from env.
pub struct ClickUpClient {
    http: reqwest::Client,
    token: String,
    space_id: String,
    done_status: String,
    billable_field: String,
    amount_field: String,
    team_id: Option<String>,
}

impl ClickUpClient {
    /// Build from env: `CLICKUP_API_TOKEN` (required), `CLICKUP_SPACE_ID`
    /// (required); `CLICKUP_DONE_STATUS`, `CLICKUP_BILLABLE_FIELD`,
    /// `CLICKUP_AMOUNT_FIELD` (optional, defaulted);
    /// `CLICKUP_TEAM_ID` (optional; required only for time tracking).
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
        let team_id = std::env::var("CLICKUP_TEAM_ID").ok().filter(|v| !v.trim().is_empty());
        Ok(Self { http: reqwest::Client::new(), token, space_id, done_status, billable_field, amount_field, team_id })
    }

    fn classify(status: reqwest::StatusCode, body: String) -> ClickUpError {
        ClickUpError::Api { status: status.as_u16(), body }
    }

    fn team(&self) -> Result<&str, ClickUpError> {
        self.team_id.as_deref().ok_or(ClickUpError::Api {
            status: 0,
            body: "CLICKUP_TEAM_ID tidak diset".into(),
        })
    }

    /// Parse ClickUp's string-or-number millisecond fields.
    fn parse_ms(value: &serde_json::Value) -> Option<i64> {
        value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
    }

    /// Build a TimeEntry from a ClickUp time-entry JSON object.
    fn parse_time_entry(value: &serde_json::Value) -> crate::clickup::client::TimeEntry {
        crate::clickup::client::TimeEntry {
            task_id: value["task"]["id"].as_str().unwrap_or_default().to_string(),
            task_name: value["task"]["name"].as_str().unwrap_or("(tanpa task)").to_string(),
            project_name: value["task"]["list"]["name"].as_str().unwrap_or("(tanpa project)").to_string(),
            duration_ms: Self::parse_ms(&value["duration"]).unwrap_or(0),
            start_ms: Self::parse_ms(&value["start"]).unwrap_or(0),
            billable: value["billable"].as_bool().unwrap_or(false),
        }
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

    async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/start", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "tid": task_id }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        Ok(())
    }

    async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/stop", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            // ClickUp returns 400 with a "no running timer" message for this case
            // — map only that to a clean None. Gating on 400 avoids swallowing
            // genuine 5xx/429 errors whose body might also mention "running".
            if status.as_u16() == 400 && body.to_lowercase().contains("running") {
                return Ok(None);
            }
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(Some(Self::parse_time_entry(&parsed["data"])))
    }

    async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/current", self.team()?);
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
        let data = &parsed["data"];
        if data.is_null() {
            return Ok(None);
        }
        Ok(Some(RunningEntry {
            task_name: data["task"]["name"].as_str().unwrap_or("(tanpa task)").to_string(),
            started_ms: Self::parse_ms(&data["start"]).unwrap_or(0),
        }))
    }

    async fn time_entries(&self, start_ms: i64, end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError> {
        let url = format!(
            "https://api.clickup.com/api/v2/team/{}/time_entries?start_date={start_ms}&end_date={end_ms}",
            self.team()?
        );
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
        let entries = parsed["data"].as_array().map(|arr| {
            arr.iter().map(Self::parse_time_entry).collect()
        }).unwrap_or_default();
        Ok(entries)
    }

    async fn add_time_entry(&self, task_id: &str, duration_ms: i64, start_ms: i64) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "tid": task_id, "duration": duration_ms, "start": start_ms }))
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
