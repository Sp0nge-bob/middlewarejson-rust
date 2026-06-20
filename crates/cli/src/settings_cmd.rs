use anyhow::Result;
use dialoguer::Password;
use middlewarejson_core::services::panel_api::{
    resolve_panel_token, PanelApiClient, PanelApiTrait, PANEL_API_BASE_URL_KEY,
    PANEL_API_TOKEN_KEY, PANEL_WEB_BASE_PATH_KEY,
};
use middlewarejson_core::services::panel_api::resolve_upstream_base_url;

use crate::ctx::AppCtx;
use crate::ui::{
    print_error, print_field, print_info, print_success, print_warning, prompt_line,
    mask_token,
};

pub fn panel_settings_show(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    let (base_url, web_path, token) = ctx.resolved_panel_settings(&repo)?;
    let balancers = repo.list_balancers()?;
    let assignments = repo.list_group_assignments()?;

    println!();
    print_field("URL панели", &base_url);
    print_field("Web base path", &web_path);
    print_field("API token", &mask_token(&token));
    print_field("Балансировщиков", &balancers.len().to_string());
    print_field("Привязок к группам", &assignments.len().to_string());
    print_env_override_hints(ctx);
    Ok(())
}

pub fn script_settings_show(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    let balancers = repo.list_balancers()?;
    let upstream_base = resolve_upstream_base_url(
        &ctx.settings,
        repo.get_setting(PANEL_API_BASE_URL_KEY)?.as_deref(),
    );
    let upstream_path = ctx.settings.upstream_json_path.trim_end_matches('/');
    let mode = ctx.settings.transform_mode.trim().to_lowercase();

    println!();
    print_field(
        "Агент",
        &format!("{}:{}", ctx.settings.agent_host, ctx.settings.agent_port),
    );
    let agent_path = ctx.settings.resolved_agent_json_path();
    print_field(
        "URL агента",
        &format!("http://{}:{}", ctx.settings.agent_host, ctx.settings.agent_port),
    );
    print_field("Путь подписки (агент)", &format!("{agent_path}/<sub_id>"));
    print_field("Режим трансформации", &ctx.settings.transform_mode);
    print_field("База данных", &ctx.settings.db_path.display().to_string());
    print_field("Upstream", &format!("{upstream_base}{upstream_path}/<sub_id>"));
    let startup_sync = if ctx.settings.panel_sync_on_startup {
        "да"
    } else {
        "нет"
    };
    let interval = if ctx.settings.panel_sync_interval.trim().is_empty() {
        "выкл"
    } else {
        ctx.settings.panel_sync_interval.trim()
    };
    print_field("Синхр. при старте", startup_sync);
    print_field("Синхр. интервал", interval);
    print_info("Параметры скрипта задаются в .env — после изменений перезапустите службу");
    if !balancers.is_empty() && mode != "rules" {
        print_warning(
            "Балансировщики не применяются в подписке. \
             Установите TRANSFORM_MODE=rules в .env и перезапустите сервер.",
        );
    }
    Ok(())
}

pub fn settings_set(
    ctx: &AppCtx,
    panel_token: Option<&str>,
    panel_base_path: Option<&str>,
) -> Result<()> {
    let repo = ctx.repo()?;
    if let Some(token) = panel_token {
        repo.set_setting(PANEL_API_TOKEN_KEY, token.trim())?;
        print_success("Panel API token сохранён");
    }
    if let Some(path) = panel_base_path {
        repo.set_setting(PANEL_WEB_BASE_PATH_KEY, path.trim())?;
        print_success(&format!("panel_web_base_path = {}", path.trim()));
    }
    Ok(())
}

pub fn edit_panel_settings(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    loop {
        let (base_url, web_path, token) = ctx.resolved_panel_settings(&repo)?;
        println!();
        print_field("URL панели", &base_url);
        print_field("Web base path", &web_path);
        print_field("API token", &mask_token(&token));
        print_env_override_hints(ctx);

        println!();
        crate::ui::print_menu_item(1, "Изменить URL панели");
        crate::ui::print_menu_item(2, "Изменить web base path");
        crate::ui::print_menu_item(3, "Изменить API token");
        crate::ui::print_menu_item(0, "Назад");

        let choice = prompt_line("Выбор [0 — назад]");
        if choice.is_empty() || choice == "0" {
            return Ok(());
        }
        match choice.as_str() {
            "1" => {
                if !ctx.settings.panel_api_base_url.is_empty() {
                    print_warning("Сначала уберите PANEL_API_BASE_URL из .env");
                    continue;
                }
                let new_url = prompt_line(&format!("URL панели [{base_url}]"));
                let new_url = if new_url.is_empty() { base_url } else { new_url };
                if !new_url.is_empty() {
                    repo.set_setting(PANEL_API_BASE_URL_KEY, &new_url)?;
                    print_success("URL панели сохранён");
                }
            }
            "2" => {
                if !ctx.settings.panel_web_base_path.is_empty() {
                    print_warning("Сначала уберите PANEL_WEB_BASE_PATH из .env");
                    continue;
                }
                let new_path = prompt_line(&format!("Web base path панели [{web_path}]"));
                repo.set_setting(PANEL_WEB_BASE_PATH_KEY, &new_path)?;
                print_success("Web base path сохранён");
            }
            "3" => {
                if !ctx.settings.panel_api_token.is_empty() {
                    print_warning("Сначала уберите PANEL_API_TOKEN из .env");
                    continue;
                }
                let new_token = Password::new()
                    .with_prompt("API token")
                    .interact()
                    .unwrap_or_default();
                if !new_token.is_empty() {
                    repo.set_setting(PANEL_API_TOKEN_KEY, &new_token)?;
                    print_success("API token сохранён");
                }
            }
            _ => print_warning("Неизвестный пункт"),
        }
    }
}

pub async fn panel_test(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    let token = ensure_panel_token(ctx, &repo, true)?;
    let Some(token) = token else {
        return Ok(());
    };

    let (base_url, web_path, _) = ctx.resolved_panel_settings(&repo)?;
    let client = PanelApiClient::new(
        ctx.settings.clone(),
        &token,
        Some(&web_path),
        Some(&base_url),
    )?;
    let result = client.probe_connection().await;

    println!();
    print_field("Запрос", &format!("{} {}", result.method, result.url));
    print_field("Ответ", &result.summary);
    print_field("Время", &format!("{:.0} мс", result.elapsed_ms));

    if result.ok {
        print_success(&format!(
            "Подключение OK — {} инбаундов",
            result.inbound_count.unwrap_or(0)
        ));
    } else {
        print_error(result.error.as_deref().unwrap_or("Подключение не удалось"));
    }
    Ok(())
}

pub fn ensure_panel_token(
    ctx: &AppCtx,
    repo: &middlewarejson_core::db::CatalogRepository,
    prompt: bool,
) -> Result<Option<String>> {
    let token = resolve_panel_token(
        &ctx.settings,
        repo.get_setting(PANEL_API_TOKEN_KEY)?.as_deref(),
    );
    if !token.is_empty() {
        return Ok(Some(token));
    }
    if !prompt {
        return Ok(None);
    }
    print_warning(
        "Panel API не настроен. Нужны base path и API token из 3x-ui → Settings → Security.",
    );
    let base_path = prompt_line(&format!(
        "Web base path панели [{}]",
        ctx.settings.panel_web_base_path
    ));
    if !base_path.is_empty() {
        repo.set_setting(PANEL_WEB_BASE_PATH_KEY, &base_path)?;
    }
    let token = Password::new()
        .with_prompt("API token")
        .interact()
        .unwrap_or_default();
    if token.is_empty() {
        print_error("Token обязателен");
        return Ok(None);
    }
    repo.set_setting(PANEL_API_TOKEN_KEY, &token)?;
    print_success("Panel API token сохранён");
    Ok(Some(resolve_panel_token(
        &ctx.settings,
        repo.get_setting(PANEL_API_TOKEN_KEY)?.as_deref(),
    )))
}

fn print_env_override_hints(ctx: &AppCtx) {
    if !ctx.settings.panel_api_base_url.is_empty() {
        print_warning("URL панели задан в .env — имеет приоритет над базой");
    }
    if !ctx.settings.panel_web_base_path.is_empty() {
        print_warning("Web base path задан в .env — имеет приоритет над базой");
    }
    if !ctx.settings.panel_api_token.is_empty() {
        print_warning("API token задан в .env — имеет приоритет над базой");
    }
}