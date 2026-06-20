use anyhow::Result;

use crate::ctx::AppCtx;
use crate::settings_cmd::ensure_panel_token;
use crate::ui::{print_error, print_info, print_success};

pub async fn sync_all(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    if ensure_panel_token(ctx, &repo, true)?.is_none() {
        return Ok(());
    }

    print_info("Синхронизация каталога инбаундов…");
    match middlewarejson_core::services::catalog_sync::sync_catalog(&ctx.settings, &repo).await {
        Ok(catalog_result) => print_success(&format!(
            "Каталог: {} эндпоинтов из {} инбаундов панели (обновлено={}, деактивировано={})",
            catalog_result.get("total_active").copied().unwrap_or(0),
            catalog_result.get("panel_inbounds").copied().unwrap_or(0),
            catalog_result.get("upserted").copied().unwrap_or(0),
            catalog_result.get("deactivated").copied().unwrap_or(0),
        )),
        Err(error) => {
            print_error(&format!("Синхронизация каталога не удалась: {error}"));
            return Ok(());
        }
    }

    println!();
    print_info("Синхронизация клиентов и групп…");
    match middlewarejson_core::services::client_sync::sync_clients(&ctx.settings, &repo).await {
        Ok(clients_result) => print_success(&format!(
            "Клиенты: {} (групп в панели {}, с клиентами {}, удалено={})",
            clients_result.get("upserted").copied().unwrap_or(0),
            clients_result.get("groups").copied().unwrap_or(0),
            clients_result.get("groups_with_clients").copied().unwrap_or(0),
            clients_result.get("removed").copied().unwrap_or(0),
        )),
        Err(error) => print_error(&format!("Синхронизация клиентов не удалась: {error}")),
    }
    Ok(())
}