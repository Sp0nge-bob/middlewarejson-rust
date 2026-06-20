use std::sync::LazyLock;
use std::time::Duration;

use tokio::sync::Mutex;

use anyhow::{anyhow, Result};
use regex::Regex;

use crate::config::Settings;
use crate::db::{CatalogRepository, Database};
use crate::services::catalog_sync::sync_catalog;
use crate::services::client_sync::sync_clients;
use crate::services::panel_api::{resolve_panel_token, PANEL_API_TOKEN_KEY};

static SYNC_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INTERVAL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(\d+)([hdm])$").expect("valid interval regex"));

pub const MIN_SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);

pub fn parse_sync_interval(value: &str) -> Result<Option<Duration>> {
    let raw = value.trim().to_lowercase();
    if raw.is_empty() {
        return Ok(None);
    }

    let captures = INTERVAL_PATTERN
        .captures(&raw)
        .ok_or_else(|| anyhow!("invalid sync interval '{value}', expected format like 30m, 12h, 24h, 7d"))?;
    let amount: u64 = captures
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .ok_or_else(|| anyhow!("invalid sync interval '{value}', amount must be positive"))?;
    if amount == 0 {
        return Err(anyhow!("invalid sync interval '{value}', amount must be positive"));
    }

    let unit = captures.get(2).map(|m| m.as_str().to_ascii_lowercase()).unwrap_or_default();
    let duration = match unit.as_str() {
        "m" => Duration::from_secs(amount * 60),
        "h" => Duration::from_secs(amount * 3600),
        "d" => Duration::from_secs(amount * 86400),
        _ => return Err(anyhow!("invalid sync interval '{value}'")),
    };
    Ok(Some(duration))
}

pub async fn run_panel_sync(settings: &Settings, reason: &str) -> bool {
    let repository = match CatalogRepository::new(Database::new(&settings.db_path)) {
        Ok(repo) => repo,
        Err(error) => {
            tracing::error!("panel sync failed ({reason}): {error}");
            return false;
        }
    };

    let token = resolve_panel_token(
        settings,
        repository
            .get_setting(PANEL_API_TOKEN_KEY)
            .ok()
            .flatten()
            .as_deref(),
    );
    if token.is_empty() {
        tracing::warn!(
            "panel sync skipped ({reason}): Panel API token is not configured"
        );
        return false;
    }

    let _guard = SYNC_LOCK.lock().await;

    match sync_catalog(settings, &repository).await {
        Ok(catalog_result) => match sync_clients(settings, &repository).await {
            Ok(clients_result) => {
                tracing::info!(
                    "panel sync ok ({reason}): catalog_active={} catalog_upserted={} \
                     clients={} groups={} groups_with_clients={} removed={}",
                    catalog_result.get("total_active").copied().unwrap_or(0),
                    catalog_result.get("upserted").copied().unwrap_or(0),
                    clients_result.get("upserted").copied().unwrap_or(0),
                    clients_result.get("groups").copied().unwrap_or(0),
                    clients_result.get("groups_with_clients").copied().unwrap_or(0),
                    clients_result.get("removed").copied().unwrap_or(0),
                );
                true
            }
            Err(error) => {
                tracing::error!("panel sync failed ({reason}): {error}");
                false
            }
        },
        Err(error) => {
            tracing::error!("panel sync failed ({reason}): {error}");
            false
        }
    }
}