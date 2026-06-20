use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::ctx::AppCtx;
use crate::settings_cmd::ensure_panel_token;
use crate::ui::{print_error, print_success, print_warning};
use middlewarejson_core::services::client_sync::sync_clients;

pub fn group_list(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    let groups = repo.list_groups()?;
    if groups.is_empty() {
        print_warning("Групп нет. Выполните синхронизацию (п. 9 в меню).");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Группа", "Клиентов", "Балансировщики"]);
    for group_name in &groups {
        let clients = repo.list_clients_by_group(group_name)?;
        let balancers = repo.list_balancers_for_group(group_name)?;
        let balancer_label = if balancers.is_empty() {
            "—".to_string()
        } else {
            balancers.join(", ")
        };
        table.add_row(vec![
            group_name.clone(),
            clients.len().to_string(),
            balancer_label,
        ]);
    }
    println!("{table}");
    Ok(())
}

pub async fn group_sync(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    if ensure_panel_token(ctx, &repo, true)?.is_none() {
        return Ok(());
    }
    match sync_clients(&ctx.settings, &repo).await {
        Ok(result) => print_success(&format!(
            "Клиенты: {} (групп в панели {}, с клиентами {}, удалено={})",
            result.get("upserted").copied().unwrap_or(0),
            result.get("groups").copied().unwrap_or(0),
            result.get("groups_with_clients").copied().unwrap_or(0),
            result.get("removed").copied().unwrap_or(0),
        )),
        Err(error) => print_error(&format!("sync failed: {error}")),
    }
    Ok(())
}

pub fn group_assign(ctx: &AppCtx, group: &str, balancer: &str) -> Result<()> {
    let repo = ctx.repo()?;
    repo.assign_group_balancer(group.trim(), balancer.trim())?;
    print_success(&format!(
        "Группа '{}' → балансировщик '{}'",
        group.trim(),
        balancer.trim()
    ));
    Ok(())
}

pub fn group_unassign(ctx: &AppCtx, group: &str) -> Result<()> {
    let repo = ctx.repo()?;
    if repo.unassign_group_balancer(group.trim(), None)? {
        print_success(&format!("Привязка снята для группы '{}'", group.trim()));
    } else {
        print_warning(&format!("Группа '{}' не была привязана", group.trim()));
    }
    Ok(())
}

pub fn group_show(ctx: &AppCtx, group: &str) -> Result<()> {
    let repo = ctx.repo()?;
    let clients = repo.list_clients_by_group(group.trim())?;
    if clients.is_empty() {
        print_warning(&format!("Клиентов в группе '{}' нет", group.trim()));
        return Ok(());
    }
    let balancers = repo.list_balancers_for_group(group.trim())?;
    let balancer_label = if balancers.is_empty() {
        "—".to_string()
    } else {
        balancers.join(", ")
    };
    println!("{} — балансировщики: {balancer_label}", group.trim());
    for client in clients {
        let status = if client.enable { "on" } else { "off" };
        let email = if client.email.is_empty() {
            "(no email)"
        } else {
            &client.email
        };
        println!("  {email}  sub_id={}  [{status}]", client.sub_id);
    }
    Ok(())
}