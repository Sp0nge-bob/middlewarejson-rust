use std::env;
use std::path::PathBuf;

use dotenvy::dotenv;

#[derive(Debug, Clone)]
pub struct Settings {
    pub upstream_base_url: String,
    pub upstream_json_path: String,
    pub agent_json_path: String,
    pub request_timeout_sec: f64,
    pub upstream_verify_ssl: bool,
    pub upstream_host_header: String,
    pub agent_host: String,
    pub agent_port: u16,
    pub transform_mode: String,
    pub db_path: PathBuf,
    pub panel_api_base_url: String,
    pub panel_web_base_path: String,
    pub panel_api_token: String,
    pub panel_verify_ssl: Option<bool>,
    pub panel_sync_on_startup: bool,
    pub panel_sync_interval: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            upstream_base_url: String::new(),
            upstream_json_path: "/json".to_string(),
            agent_json_path: String::new(),
            request_timeout_sec: 15.0,
            upstream_verify_ssl: true,
            upstream_host_header: String::new(),
            agent_host: "127.0.0.1".to_string(),
            agent_port: 8080,
            transform_mode: "passthrough".to_string(),
            db_path: PathBuf::from("data/middleware.db"),
            panel_api_base_url: String::new(),
            panel_web_base_path: String::new(),
            panel_api_token: String::new(),
            panel_verify_ssl: None,
            panel_sync_on_startup: true,
            panel_sync_interval: "24h".to_string(),
        }
    }
}

impl Settings {
    pub fn from_env() -> Self {
        let _ = dotenv();
        let mut settings = Self::default();
        settings.apply_env_overrides();
        settings
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = env::var("UPSTREAM_BASE_URL") {
            self.upstream_base_url = v;
        }
        if let Ok(v) = env::var("UPSTREAM_JSON_PATH") {
            self.upstream_json_path = v;
        }
        if let Ok(v) = env::var("AGENT_JSON_PATH") {
            self.agent_json_path = v;
        }
        if let Ok(v) = env::var("REQUEST_TIMEOUT_SEC") {
            if let Ok(n) = v.parse() {
                self.request_timeout_sec = n;
            }
        }
        if let Ok(v) = env::var("UPSTREAM_VERIFY_SSL") {
            self.upstream_verify_ssl = parse_bool(&v, true);
        }
        if let Ok(v) = env::var("UPSTREAM_HOST_HEADER") {
            self.upstream_host_header = v;
        }
        if let Ok(v) = env::var("AGENT_HOST") {
            self.agent_host = v;
        }
        if let Ok(v) = env::var("AGENT_PORT") {
            if let Ok(n) = v.parse() {
                self.agent_port = n;
            }
        }
        if let Ok(v) = env::var("TRANSFORM_MODE") {
            self.transform_mode = v;
        }
        if let Ok(v) = env::var("DB_PATH") {
            self.db_path = PathBuf::from(v);
        }
        if let Ok(v) = env::var("PANEL_API_BASE_URL") {
            self.panel_api_base_url = v;
        }
        if let Ok(v) = env::var("PANEL_WEB_BASE_PATH") {
            self.panel_web_base_path = v;
        }
        if let Ok(v) = env::var("PANEL_API_TOKEN") {
            self.panel_api_token = v;
        }
        if let Ok(v) = env::var("PANEL_VERIFY_SSL") {
            self.panel_verify_ssl = Some(parse_bool(&v, true));
        }
        if let Ok(v) = env::var("PANEL_SYNC_ON_STARTUP") {
            self.panel_sync_on_startup = parse_bool(&v, true);
        }
        if let Ok(v) = env::var("PANEL_SYNC_INTERVAL") {
            self.panel_sync_interval = v;
        }
    }

    pub fn resolved_agent_json_path(&self) -> String {
        let explicit = self.agent_json_path.trim();
        if !explicit.is_empty() {
            return explicit.trim_end_matches('/').to_string();
        }
        self.upstream_json_path.trim_end_matches('/').to_string()
    }

    pub fn resolved_panel_base_url(&self) -> String {
        let base = if self.panel_api_base_url.is_empty() {
            &self.upstream_base_url
        } else {
            &self.panel_api_base_url
        };
        base.trim_end_matches('/').to_string()
    }

    pub fn resolved_panel_verify_ssl(&self) -> bool {
        self.panel_verify_ssl
            .unwrap_or(self.upstream_verify_ssl)
    }
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        "" => default,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_agent_json_path_prefers_explicit() {
        let settings = Settings {
            agent_json_path: "/custom".to_string(),
            upstream_json_path: "/json".to_string(),
            ..Default::default()
        };
        assert_eq!(settings.resolved_agent_json_path(), "/custom");
    }

    #[test]
    fn resolved_panel_base_url_falls_back_to_upstream() {
        let settings = Settings {
            upstream_base_url: "https://upstream.example.com".to_string(),
            ..Default::default()
        };
        assert_eq!(
            settings.resolved_panel_base_url(),
            "https://upstream.example.com"
        );
    }
}