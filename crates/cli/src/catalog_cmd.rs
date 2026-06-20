use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use middlewarejson_core::services::catalog_sync::sync_catalog;

use crate::ctx::AppCtx;
use crate::settings_cmd::ensure_panel_token;
use crate::ui::{print_error, print_info, print_success, print_warning, CANCEL_HINT};

pub fn display_remarks(value: &str, max_len: usize) -> String {
    let cleaned = regex::Regex::new(r"[\u{1F1E6}-\u{1F1FF}]{2}\s*")
        .ok()
        .map(|re| re.replace(value, "").trim().to_string())
        .unwrap_or_else(|| value.to_string());
    if cleaned.chars().count() > max_len {
        format!("{}…", cleaned.chars().take(max_len - 1).collect::<String>())
    } else {
        cleaned
    }
}

fn format_endpoint(row: &serde_json::Map<String, serde_json::Value>) -> String {
    let address = row
        .get("address")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let port = row.get("port").and_then(|v| v.as_i64()).unwrap_or(0);
    match (address.is_empty(), port) {
        (false, 0) => address.to_string(),
        (false, p) => format!("{address}:{p}"),
        (true, p) if p > 0 => format!("порт {p}"),
        _ => "—".to_string(),
    }
}

pub fn print_inbound_table(rows: &[serde_json::Map<String, serde_json::Value>], for_selection: bool) {
    let active_count = rows.iter().filter(|r| r.get("is_active").and_then(|v| v.as_bool()).unwrap_or(false)).count();
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["#", "ID", "Статус", "Название", "Прот.", "Эндпоинт", "Сеть"]);

    for (index, row) in rows.iter().enumerate() {
        let panel_id = row
            .get("panel_inbound_id")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".to_string());
        let is_active = row.get("is_active").and_then(|v| v.as_bool()).unwrap_or(false);
        let status = if is_active { "вкл" } else { "выкл" };
        table.add_row(vec![
            Cell::new(index.to_string()),
            Cell::new(panel_id),
            Cell::new(status),
            Cell::new(display_remarks(
                row.get("remarks").and_then(|v| v.as_str()).unwrap_or(""),
                20,
            )),
            Cell::new(row.get("protocol").and_then(|v| v.as_str()).unwrap_or("—")),
            Cell::new(format_endpoint(row)),
            Cell::new(row.get("network").and_then(|v| v.as_str()).unwrap_or("—")),
        ]);
    }

    println!("{table}");
    print_info(&format!("всего {} | активных {active_count}", rows.len()));
    if for_selection {
        print_info(&format!(
            "Выбирайте номера из колонки # (0, 1, 2…), не ID из панели. {CANCEL_HINT}"
        ));
    }
}

pub async fn catalog_sync(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    if ensure_panel_token(ctx, &repo, true)?.is_none() {
        return Ok(());
    }
    match sync_catalog(&ctx.settings, &repo).await {
        Ok(result) => {
            print_success(&format!(
                "Synced {} endpoints from {} panel inbounds (upserted={}, deactivated={})",
                result.get("total_active").copied().unwrap_or(0),
                result.get("panel_inbounds").copied().unwrap_or(0),
                result.get("upserted").copied().unwrap_or(0),
                result.get("deactivated").copied().unwrap_or(0),
            ));
        }
        Err(error) => print_error(&format!("sync failed: {error}")),
    }
    Ok(())
}

pub fn catalog_list(ctx: &AppCtx, active_only: bool, for_selection: bool) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    let repo = ctx.repo()?;
    let rows = repo.list_inbounds(active_only)?;
    if rows.is_empty() {
        print_warning("Каталог пуст. Выполните синхронизацию (п. 9 в меню).");
        return Ok(vec![]);
    }
    print_inbound_table(&rows, for_selection);
    Ok(rows)
}