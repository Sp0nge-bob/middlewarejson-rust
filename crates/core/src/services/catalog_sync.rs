use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;

use crate::config::Settings;
use crate::db::repository::CatalogRepository;
use crate::models::inbound::InboundDescriptor;
use crate::models::panel_inbound::panel_inbounds_to_descriptors;
use crate::services::panel_api::{
    resolve_panel_base_url, resolve_panel_token, resolve_panel_web_base_path, PanelApiClient,
    PanelApiError, PanelApiTrait, PANEL_API_BASE_URL_KEY, PANEL_API_TOKEN_KEY,
    PANEL_WEB_BASE_PATH_KEY,
};

pub fn configs_to_inbounds(configs: &[Value]) -> Vec<InboundDescriptor> {
    configs
        .iter()
        .enumerate()
        .filter_map(|(index, config)| InboundDescriptor::from_config(index, config))
        .collect()
}

pub async fn fetch_panel_inbounds(
    settings: &Settings,
    repository: &CatalogRepository,
) -> Result<Vec<serde_json::Map<String, Value>>> {
    let token = resolve_panel_token(
        settings,
        repository.get_setting(PANEL_API_TOKEN_KEY)?.as_deref(),
    );
    let web_path = resolve_panel_web_base_path(
        settings,
        repository
            .get_setting(PANEL_WEB_BASE_PATH_KEY)?
            .as_deref(),
    );
    let base_url = resolve_panel_base_url(
        settings,
        repository
            .get_setting(PANEL_API_BASE_URL_KEY)?
            .as_deref(),
    );
    let client = PanelApiClient::new(
        settings.clone(),
        &token,
        Some(&web_path),
        Some(&base_url),
    )?;
    client.fetch_inbounds_list().await.map_err(map_panel_error)
}

pub async fn sync_catalog(
    settings: &Settings,
    repository: &CatalogRepository,
) -> Result<HashMap<&'static str, usize>> {
    let inbounds_raw = fetch_panel_inbounds(settings, repository).await?;
    let descriptors: Vec<InboundDescriptor> = panel_inbounds_to_descriptors(
        &inbounds_raw
            .iter()
            .map(|item| Value::Object(item.clone()))
            .collect::<Vec<_>>(),
    );
    let (upserted, deactivated) = repository.upsert_inbounds(&descriptors)?;

    Ok(HashMap::from([
        ("panel_inbounds", inbounds_raw.len()),
        ("upserted", upserted),
        ("deactivated", deactivated),
        ("total_active", descriptors.len()),
    ]))
}

fn map_panel_error(error: PanelApiError) -> anyhow::Error {
    anyhow::Error::new(error)
}