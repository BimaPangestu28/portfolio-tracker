use async_trait::async_trait;

/// A ClickUp List, surfaced to the assistant as a "project".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// Fields for creating a task. Phase 1 uses title + optional due (epoch ms).
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub name: String,
    pub due_date_ms: Option<i64>,
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
}

/// Real reqwest-backed client. Reads token + space id from env.
pub struct ClickUpClient {
    http: reqwest::Client,
    token: String,
    space_id: String,
}

impl ClickUpClient {
    /// Build from env: `CLICKUP_API_TOKEN` (required), `CLICKUP_SPACE_ID`
    /// (required). `CLICKUP_WORKSPACE_ID` is accepted for documentation but the
    /// v2 endpoints used here are space-scoped, so it is not required.
    pub fn from_env() -> Result<Self, ClickUpError> {
        let token = std::env::var("CLICKUP_API_TOKEN").map_err(|_| ClickUpError::NoToken)?;
        if token.trim().is_empty() {
            return Err(ClickUpError::NoToken);
        }
        let space_id = std::env::var("CLICKUP_SPACE_ID")
            .map_err(|_| ClickUpError::Api { status: 0, body: "CLICKUP_SPACE_ID tidak diset".into() })?;
        Ok(Self { http: reqwest::Client::new(), token, space_id })
    }

    fn classify(status: reqwest::StatusCode, body: String) -> ClickUpError {
        ClickUpError::Api { status: status.as_u16(), body }
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
