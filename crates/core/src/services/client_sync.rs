use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde_json::Map;

use crate::config::Settings;
use crate::db::repository::{CatalogRepository, ClientRecord};
use crate::services::panel_api::{
    parse_group_names, resolve_panel_base_url, resolve_panel_token,
    resolve_panel_web_base_path, PanelApiClient, PanelApiTrait,
    PANEL_API_BASE_URL_KEY, PANEL_API_TOKEN_KEY, PANEL_WEB_BASE_PATH_KEY,
};

async fn make_panel_client(
    settings: &Settings,
    repository: &CatalogRepository,
) -> Result<PanelApiClient> {
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
    PanelApiClient::new(
        settings.clone(),
        &token,
        Some(&web_path),
        Some(&base_url),
    )
    .map_err(|e| anyhow::Error::new(e))
}

fn build_email_lookup(clients: &[Map<String, serde_json::Value>]) -> HashMap<String, Map<String, serde_json::Value>> {
    let mut lookup = HashMap::new();
    for row in clients {
        let email = row
            .get("email")
            .map(value_to_string)
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if !email.is_empty() {
            lookup.insert(email, row.clone());
        }
    }
    lookup
}

fn extract_email(member: &Map<String, serde_json::Value>) -> String {
    member
        .get("email")
        .or_else(|| member.get("Email"))
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string()
}

async fn resolve_client_record(
    group_name: &str,
    member: &Map<String, serde_json::Value>,
    email_lookup: &HashMap<String, Map<String, serde_json::Value>>,
    panel_client: &PanelApiClient,
) -> Result<Option<ClientRecord>> {
    let email = extract_email(member);
    if email.is_empty() {
        return Ok(None);
    }

    let mut sub_id = member
        .get("subId")
        .or_else(|| member.get("sub_id"))
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut enable = member.get("enable") != Some(&serde_json::Value::Bool(false));

    if sub_id.is_empty() {
        if let Some(cached) = email_lookup.get(&email.to_lowercase()) {
            sub_id = cached
                .get("subId")
                .or_else(|| cached.get("sub_id"))
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            enable = cached.get("enable") != Some(&serde_json::Value::Bool(false));
        }
    }

    if sub_id.is_empty() {
        if let Some(detail) = panel_client.fetch_client_by_email(&email).await? {
            sub_id = detail
                .get("subId")
                .or_else(|| detail.get("sub_id"))
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            enable = detail.get("enable") != Some(&serde_json::Value::Bool(false));
        }
    }

    if sub_id.is_empty() || !enable {
        return Ok(None);
    }

    Ok(Some(ClientRecord {
        sub_id,
        group_name: group_name.to_string(),
        email,
        enable: true,
    }))
}

pub async fn collect_clients_from_groups(
    panel_client: &PanelApiClient,
) -> Result<(Vec<ClientRecord>, Vec<String>)> {
    let groups_raw = panel_client.fetch_groups().await?;
    let group_names = parse_group_names(&groups_raw);
    let clients_list = panel_client.fetch_clients_list().await?;
    let email_lookup = build_email_lookup(&clients_list);

    let mut clients = Vec::new();
    let mut seen_sub_ids = HashSet::new();

    for group_name in &group_names {
        let members = panel_client.fetch_group_emails(group_name).await?;
        for member in members {
            let record = resolve_client_record(group_name, &member, &email_lookup, panel_client).await?;
            let Some(record) = record else {
                continue;
            };
            if seen_sub_ids.contains(&record.sub_id) {
                continue;
            }
            seen_sub_ids.insert(record.sub_id.clone());
            clients.push(record);
        }
    }

    Ok((clients, group_names))
}

pub async fn sync_clients(
    settings: &Settings,
    repository: &CatalogRepository,
) -> Result<HashMap<&'static str, usize>> {
    let panel_client = make_panel_client(settings, repository).await?;
    let (clients, group_names) = collect_clients_from_groups(&panel_client).await?;

    let groups_count = group_names.len();
    repository.upsert_panel_groups(&group_names)?;
    let (upserted, removed) = repository.upsert_clients(&clients)?;
    let groups_with_clients = clients
        .iter()
        .filter(|client| !client.group_name.is_empty())
        .map(|client| client.group_name.clone())
        .collect::<HashSet<_>>()
        .len();

    Ok(HashMap::from([
        ("groups", groups_count),
        ("groups_with_clients", groups_with_clients),
        ("panel_clients", clients.len()),
        ("upserted", upserted),
        ("removed", removed),
    ]))
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        v => v.to_string(),
    }
}