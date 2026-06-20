use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use thiserror::Error;

use crate::config::Settings;

pub const PASSTHROUGH_REQUEST_HEADERS: &[&str] =
    &["user-agent", "accept", "accept-language"];
pub const PASSTHROUGH_RESPONSE_HEADERS: &[&str] = &[
    "subscription-userinfo",
    "profile-update-interval",
    "profile-title",
    "support-url",
    "profile-web-page-url",
    "announce",
    "routing-enable",
    "routing",
];

#[derive(Debug, Clone)]
pub struct UpstreamResult {
    pub status_code: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("upstream timeout")]
    Timeout,
    #[error("upstream unavailable")]
    Unavailable,
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait UpstreamClientTrait: Send + Sync {
    fn build_url(&self, sub_id: &str) -> String;
    async fn fetch(
        &self,
        sub_id: &str,
        query_string: &str,
        request_headers: Option<&HashMap<String, String>>,
    ) -> Result<UpstreamResult, UpstreamError>;
}

pub struct UpstreamClient {
    settings: Settings,
    base_url: String,
    client: Client,
}

impl UpstreamClient {
    pub fn new(settings: Settings, base_url: Option<&str>) -> Result<Self, UpstreamError> {
        let base_url = base_url
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| settings.upstream_base_url.trim_end_matches('/').to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs_f64(settings.request_timeout_sec))
            .danger_accept_invalid_certs(!settings.upstream_verify_ssl)
            .build()
            .map_err(|e| UpstreamError::Other(e.to_string()))?;

        Ok(Self {
            settings,
            base_url,
            client,
        })
    }
}

#[async_trait]
impl UpstreamClientTrait for UpstreamClient {
    fn build_url(&self, sub_id: &str) -> String {
        let path = self.settings.upstream_json_path.trim_end_matches('/');
        format!("{}{path}/{sub_id}", self.base_url)
    }

    async fn fetch(
        &self,
        sub_id: &str,
        query_string: &str,
        request_headers: Option<&HashMap<String, String>>,
    ) -> Result<UpstreamResult, UpstreamError> {
        let mut url = self.build_url(sub_id);
        if !query_string.is_empty() {
            url = format!("{url}?{query_string}");
        }

        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(request_headers) = request_headers {
            for (key, value) in request_headers {
                if PASSTHROUGH_REQUEST_HEADERS.contains(&key.to_lowercase().as_str()) {
                    if let (Ok(name), Ok(val)) = (
                        reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                        reqwest::header::HeaderValue::from_str(value),
                    ) {
                        headers.insert(name, val);
                    }
                }
            }
        }
        if !self.settings.upstream_host_header.is_empty() {
            if let Ok(val) =
                reqwest::header::HeaderValue::from_str(&self.settings.upstream_host_header)
            {
                headers.insert(reqwest::header::HOST, val);
            }
        }

        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    UpstreamError::Timeout
                } else {
                    UpstreamError::Unavailable
                }
            })?;

        let status_code = response.status().as_u16();
        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                let key = name.as_str().to_lowercase();
                if PASSTHROUGH_RESPONSE_HEADERS.contains(&key.as_str()) {
                    value.to_str().ok().map(|v| (key, v.to_string()))
                } else {
                    None
                }
            })
            .collect();
        let body = response
            .text()
            .await
            .map_err(|e| UpstreamError::Other(e.to_string()))?;

        Ok(UpstreamResult {
            status_code,
            body,
            headers: response_headers,
        })
    }
}