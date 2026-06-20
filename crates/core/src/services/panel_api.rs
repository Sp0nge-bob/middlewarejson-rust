use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use thiserror::Error;
use url::Url;

use crate::config::Settings;

pub const PANEL_API_TOKEN_KEY: &str = "panel_api_token";
pub const PANEL_WEB_BASE_PATH_KEY: &str = "panel_web_base_path";
pub const PANEL_API_BASE_URL_KEY: &str = "panel_api_base_url";

pub fn resolve_panel_token(settings: &Settings, repository_token: Option<&str>) -> String {
    if !settings.panel_api_token.is_empty() {
        return settings.panel_api_token.trim().to_string();
    }
    repository_token.unwrap_or("").trim().to_string()
}

pub fn resolve_panel_web_base_path(
    settings: &Settings,
    repository_value: Option<&str>,
) -> String {
    if !settings.panel_web_base_path.is_empty() {
        return settings.panel_web_base_path.trim().to_string();
    }
    repository_value.unwrap_or("").trim().to_string()
}

pub fn resolve_panel_base_url(settings: &Settings, repository_value: Option<&str>) -> String {
    if !settings.panel_api_base_url.is_empty() {
        return settings.panel_api_base_url.trim().trim_end_matches('/').to_string();
    }
    if let Some(value) = repository_value {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }
    settings.upstream_base_url.trim_end_matches('/').to_string()
}

pub fn resolve_upstream_base_url(
    settings: &Settings,
    repository_panel_url: Option<&str>,
) -> String {
    let explicit = settings.upstream_base_url.trim();
    if !explicit.is_empty() {
        return explicit.trim_end_matches('/').to_string();
    }
    resolve_panel_base_url(settings, repository_panel_url)
}

pub fn parse_group_names(groups: &[Value]) -> Vec<String> {
    let mut names = Vec::new();
    for item in groups {
        let name = match item {
            Value::String(s) => s.trim().to_string(),
            Value::Object(obj) => obj
                .get("name")
                .or_else(|| obj.get("groupName"))
                .or_else(|| obj.get("group_name"))
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string(),
            _ => String::new(),
        };
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

pub fn parse_group_members(obj: &Value) -> Vec<serde_json::Map<String, Value>> {
    let Some(items) = obj.as_array() else {
        return Vec::new();
    };
    let mut members = Vec::new();
    for item in items {
        match item {
            Value::String(email) => {
                let trimmed = email.trim();
                if !trimmed.is_empty() {
                    let mut map = serde_json::Map::new();
                    map.insert("email".to_string(), Value::String(trimmed.to_string()));
                    members.push(map);
                }
            }
            Value::Object(map) => members.push(map.clone()),
            _ => {}
        }
    }
    members
}

#[derive(Debug, Error)]
pub enum PanelApiError {
    #[error("Panel API token is not set. Set PANEL_API_TOKEN in .env or via settings")]
    MissingToken,
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone)]
pub struct PanelProbeResult {
    pub method: String,
    pub url: String,
    pub status_code: Option<u16>,
    pub elapsed_ms: f64,
    pub ok: bool,
    pub summary: String,
    pub inbound_count: Option<usize>,
    pub error: Option<String>,
}

#[async_trait]
pub trait PanelApiTrait: Send + Sync {
    async fn fetch_inbounds_list(&self) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError>;
    async fn fetch_clients_list(&self) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError>;
    async fn fetch_groups(&self) -> Result<Vec<Value>, PanelApiError>;
    async fn fetch_group_emails(
        &self,
        group_name: &str,
    ) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError>;
    async fn fetch_client_by_email(
        &self,
        email: &str,
    ) -> Result<Option<serde_json::Map<String, Value>>, PanelApiError>;
    async fn probe_connection(&self) -> PanelProbeResult;
}

pub struct PanelApiClient {
    settings: Settings,
    token: String,
    web_path: String,
    api_base_url: String,
    client: Client,
}

impl PanelApiClient {
    pub fn new(
        settings: Settings,
        token: &str,
        web_base_path: Option<&str>,
        api_base_url: Option<&str>,
    ) -> Result<Self, PanelApiError> {
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(PanelApiError::MissingToken);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs_f64(settings.request_timeout_sec))
            .danger_accept_invalid_certs(!settings.resolved_panel_verify_ssl())
            .build()
            .map_err(|e| PanelApiError::Message(e.to_string()))?;

        let web_path = web_base_path
            .unwrap_or(&settings.panel_web_base_path)
            .trim_matches('/')
            .to_string();

        Ok(Self {
            settings,
            token,
            web_path,
            api_base_url: api_base_url
                .unwrap_or("")
                .trim()
                .trim_end_matches('/')
                .to_string(),
            client,
        })
    }

    fn base_url(&self) -> String {
        let base = if self.api_base_url.is_empty() {
            self.settings.resolved_panel_base_url()
        } else {
            self.api_base_url.clone()
        };
        if self.web_path.is_empty() {
            format!("{base}/")
        } else {
            format!("{base}/{}/", self.web_path)
        }
    }

    fn join_url(&self, path: &str) -> Result<String, PanelApiError> {
        let base = Url::parse(&self.base_url())
            .map_err(|e| PanelApiError::Message(format!("invalid panel base url: {e}")))?;
        base.join(path.trim_start_matches('/'))
            .map(|url| url.to_string())
            .map_err(|e| PanelApiError::Message(format!("invalid panel path: {e}")))
    }

    async fn request(&self, method: reqwest::Method, path: &str) -> Result<Value, PanelApiError> {
        let url = self.join_url(path)?;
        let response = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| PanelApiError::Message(e.to_string()))?;

        if response.status().as_u16() == 401 {
            return Err(PanelApiError::Message(
                "Panel API authentication failed (401)".to_string(),
            ));
        }

        let response = response
            .error_for_status()
            .map_err(|e| PanelApiError::Message(e.to_string()))?;
        let payload: Value = response
            .json()
            .await
            .map_err(|e| PanelApiError::Message(e.to_string()))?;

        let Some(obj) = payload.as_object() else {
            return Err(PanelApiError::Message(format!(
                "Unexpected Panel API response type: {}",
                payload_type_name(&payload)
            )));
        };

        if obj.get("success") == Some(&Value::Bool(false)) {
            let message = obj
                .get("msg")
                .or_else(|| obj.get("message"))
                .map(value_to_string)
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(PanelApiError::Message(format!("Panel API error: {message}")));
        }

        Ok(obj.get("obj").cloned().unwrap_or(Value::Null))
    }
}

#[async_trait]
impl PanelApiTrait for PanelApiClient {
    async fn fetch_inbounds_list(&self) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError> {
        let obj = self.request(reqwest::Method::GET, "/panel/api/inbounds/list").await?;
        match obj {
            Value::Null => Ok(Vec::new()),
            Value::Array(items) => Ok(items.into_iter().filter_map(|v| v.as_object().cloned()).collect()),
            _ => Err(PanelApiError::Message("inbounds/list obj is not a list".to_string())),
        }
    }

    async fn fetch_clients_list(&self) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError> {
        let obj = self.request(reqwest::Method::GET, "/panel/api/clients/list").await?;
        match obj {
            Value::Null => Ok(Vec::new()),
            Value::Array(items) => Ok(items.into_iter().filter_map(|v| v.as_object().cloned()).collect()),
            _ => Err(PanelApiError::Message("clients/list obj is not a list".to_string())),
        }
    }

    async fn fetch_groups(&self) -> Result<Vec<Value>, PanelApiError> {
        let obj = self.request(reqwest::Method::GET, "/panel/api/clients/groups").await?;
        match obj {
            Value::Null => Ok(Vec::new()),
            Value::Array(items) => Ok(items
                .into_iter()
                .filter(|item| item.is_object() || item.is_string())
                .collect()),
            _ => Err(PanelApiError::Message("clients/groups obj is not a list".to_string())),
        }
    }

    async fn fetch_group_emails(
        &self,
        group_name: &str,
    ) -> Result<Vec<serde_json::Map<String, Value>>, PanelApiError> {
        let encoded = percent_encode(group_name);
        let path = format!("/panel/api/clients/groups/{encoded}/emails");
        let obj = self.request(reqwest::Method::GET, &path).await?;
        Ok(parse_group_members(&obj))
    }

    async fn fetch_client_by_email(
        &self,
        email: &str,
    ) -> Result<Option<serde_json::Map<String, Value>>, PanelApiError> {
        let encoded = percent_encode(email);
        let url = self.join_url(&format!("panel/api/clients/get/{encoded}"))?;
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| PanelApiError::Message(e.to_string()))?;

        let status = response.status().as_u16();
        if status == 401 || status == 403 {
            return Err(PanelApiError::Message(
                "Panel API authentication failed".to_string(),
            ));
        }
        if status == 404 {
            return Ok(None);
        }

        let response = response
            .error_for_status()
            .map_err(|e| PanelApiError::Message(e.to_string()))?;
        let payload: Value = response
            .json()
            .await
            .map_err(|e| PanelApiError::Message(e.to_string()))?;

        let Some(obj) = payload.as_object() else {
            return Err(PanelApiError::Message(format!(
                "Unexpected Panel API response type: {}",
                payload_type_name(&payload)
            )));
        };
        if obj.get("success") == Some(&Value::Bool(false)) {
            return Ok(None);
        }
        Ok(obj.get("obj").and_then(|v| v.as_object().cloned()))
    }

    async fn probe_connection(&self) -> PanelProbeResult {
        let path = "/panel/api/inbounds/list";
        let url = match self.join_url(path) {
            Ok(url) => url,
            Err(error) => {
                return PanelProbeResult {
                    method: "GET".to_string(),
                    url: String::new(),
                    status_code: None,
                    elapsed_ms: 0.0,
                    ok: false,
                    summary: error.to_string(),
                    inbound_count: None,
                    error: Some(error.to_string()),
                };
            }
        };

        let started = Instant::now();
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await;

        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

        let response = match response {
            Ok(response) => response,
            Err(error) => {
                return PanelProbeResult {
                    method: "GET".to_string(),
                    url,
                    status_code: None,
                    elapsed_ms,
                    ok: false,
                    summary: format!("ошибка сети: {error}"),
                    inbound_count: None,
                    error: Some(error.to_string()),
                };
            }
        };

        let status_code = response.status().as_u16();
        if status_code == 401 {
            return PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: false,
                summary: format!("HTTP {status_code} Unauthorized"),
                inbound_count: None,
                error: Some("Panel API authentication failed (401)".to_string()),
            };
        }

        if !response.status().is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_default()
                .trim()
                .replace('\n', " ");
            let body = if body.len() > 160 {
                format!("{}...", &body[..157])
            } else {
                body
            };
            let summary = if body.is_empty() {
                format!("HTTP {status_code}")
            } else {
                format!("HTTP {status_code}, body={body}")
            };
            return PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: false,
                summary: summary.clone(),
                inbound_count: None,
                error: Some(summary),
            };
        }

        let payload: Value = match response.json().await {
            Ok(payload) => payload,
            Err(error) => {
                return PanelProbeResult {
                    method: "GET".to_string(),
                    url,
                    status_code: Some(status_code),
                    elapsed_ms,
                    ok: false,
                    summary: format!("HTTP {status_code}, не JSON"),
                    inbound_count: None,
                    error: Some(error.to_string()),
                };
            }
        };

        let Some(obj) = payload.as_object() else {
            return PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: false,
                summary: format!(
                    "HTTP {status_code}, неожиданный тип ответа: {}",
                    payload_type_name(&payload)
                ),
                inbound_count: None,
                error: Some(format!(
                    "Unexpected Panel API response type: {}",
                    payload_type_name(&payload)
                )),
            };
        };

        if obj.get("success") == Some(&Value::Bool(false)) {
            let message = obj
                .get("msg")
                .or_else(|| obj.get("message"))
                .map(value_to_string)
                .unwrap_or_else(|| "unknown error".to_string());
            return PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: false,
                summary: format!("HTTP {status_code}, success=false, msg={message}"),
                inbound_count: None,
                error: Some(format!("Panel API error: {message}")),
            };
        }

        match obj.get("obj") {
            None | Some(Value::Null) => PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: true,
                summary: format!("HTTP {status_code}, success=true, obj=null"),
                inbound_count: Some(0),
                error: None,
            },
            Some(Value::Array(items)) => {
                let inbound_count = items.len();
                PanelProbeResult {
                    method: "GET".to_string(),
                    url,
                    status_code: Some(status_code),
                    elapsed_ms,
                    ok: true,
                    summary: format!(
                        "HTTP {status_code}, success=true, obj=[{inbound_count} элементов]"
                    ),
                    inbound_count: Some(inbound_count),
                    error: None,
                }
            }
            Some(other) => PanelProbeResult {
                method: "GET".to_string(),
                url,
                status_code: Some(status_code),
                elapsed_ms,
                ok: false,
                summary: format!(
                    "HTTP {status_code}, success=true, obj={}",
                    payload_type_name(other)
                ),
                inbound_count: None,
                error: Some("inbounds/list obj is not a list".to_string()),
            },
        }
    }
}

impl PanelApiClient {
    pub async fn test_connection(&self) -> Result<usize, PanelApiError> {
        let result = self.probe_connection().await;
        if !result.ok {
            return Err(PanelApiError::Message(
                result.error.unwrap_or(result.summary),
            ));
        }
        Ok(result.inbound_count.unwrap_or(0))
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn payload_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}