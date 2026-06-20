use anyhow::Result;
use middlewarejson_core::services::systemd_service::{
    detect_project_root, detect_server_binary, get_service_scope, install_service,
    read_service_status, render_unit_file, restart_service, start_service, is_linux,
    systemctl_available,
};

use crate::ctx::AppCtx;
use crate::ui::{
    confirm_prompt, print_error, print_field, print_info, print_menu_item, print_success,
    print_warning, prompt_line,
};

fn format_active_state(active: &str) -> String {
    match active {
        "active" => "работает".to_string(),
        "inactive" => "остановлена".to_string(),
        "failed" => "ошибка".to_string(),
        "not-found" => "не установлена".to_string(),
        other => other.to_string(),
    }
}

fn probe_health(ctx: &AppCtx) -> (bool, String) {
    let url = format!(
        "http://{}:{}/health",
        ctx.settings.agent_host, ctx.settings.agent_port
    );
    match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .and_then(|c| c.get(&url).send())
    {
        Ok(response) => {
            if response.status().as_u16() == 200 {
                (true, format!("HTTP 200 — {url}"))
            } else {
                (false, format!("HTTP {} — {url}", response.status()))
            }
        }
        Err(error) => (false, format!("{url} — {error}")),
    }
}

pub fn print_service_status(ctx: &AppCtx) -> bool {
    if !is_linux() {
        print_warning("systemd доступен только на Linux VPS — соберите и запустите проект на сервере");
        return false;
    }
    if !systemctl_available() {
        print_warning("systemctl не найден в PATH");
        return false;
    }

    let status = read_service_status(None);
    if let Some(error) = &status.error {
        if !status.installed {
            print_error(error);
            return false;
        }
    }

    println!();
    let scope_label = status.scope.as_ref().map(|s| s.kind.as_str()).unwrap_or("—");
    print_field("Область", &format!("systemd ({scope_label})"));
    print_field(
        "Unit-файл",
        &status
            .unit_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "—".to_string()),
    );
    print_field("Установлена", if status.installed { "да" } else { "нет" });
    print_field("Состояние", &format_active_state(&status.active));
    if let Some(enabled) = &status.enabled {
        print_field("Автозапуск", enabled);
    }
    if let Some(pid) = &status.main_pid {
        print_field("PID", pid);
    }

    let (health_ok, health_msg) = probe_health(ctx);
    print_field("Health", &health_msg);
    if !health_ok && status.active == "active" {
        print_warning("Служба active, но /health не отвечает — проверьте порт и логи");
    }

    if !status.journal_tail.is_empty() {
        println!();
        print_info("Последние строки journal:");
        for line in status.journal_tail.iter().take(5) {
            print_info(line);
        }
    }
    if let Some(error) = &status.error {
        print_warning(error);
    }
    true
}

pub fn service_status_menu(ctx: &AppCtx) -> Result<()> {
    if !print_service_status(ctx) {
        return Ok(());
    }
    let status = read_service_status(None);
    if !status.installed {
        print_info("Установите службу через п. 5 в меню «Настройки»");
        return Ok(());
    }

    println!();
    print_menu_item(0, "Назад");
    if status.active != "active" {
        print_menu_item(1, "Запустить");
    } else {
        print_menu_item(1, "Запустить (уже работает)");
    }
    print_menu_item(2, "Перезапустить");

    let choice = prompt_line("Выбор [0 — назад]");
    if choice.is_empty() || choice == "0" {
        return Ok(());
    }
    let scope = status.scope.as_ref();
    match choice.as_str() {
        "1" => {
            let (ok, message) = start_service(scope);
            if ok {
                print_success(&message);
            } else {
                print_error(&message);
            }
        }
        "2" => {
            let (ok, message) = restart_service(scope);
            if ok {
                print_success(&message);
            } else {
                print_error(&message);
            }
        }
        _ => print_warning("Неизвестный пункт"),
    }
    Ok(())
}

pub fn install_systemd(ctx: &AppCtx) -> Result<()> {
    if !is_linux() {
        print_warning("systemd доступен только на Linux VPS — соберите и запустите проект на сервере");
        return Ok(());
    }
    if !systemctl_available() {
        print_warning("systemctl не найден в PATH");
        return Ok(());
    }

    let root = detect_project_root();
    let binary = detect_server_binary(&root);
    let env_file = root.join(".env");

    println!();
    print_field("Каталог проекта", &root.display().to_string());
    print_field("Бинарник", &binary.display().to_string());
    print_field("Env-файл", &env_file.display().to_string());

    if !binary.is_file() {
        print_error("Сначала выполните: cargo build --release");
        return Ok(());
    }
    if !env_file.is_file() {
        print_error("Создайте .env (cp .env.example .env)");
        return Ok(());
    }

    let scope = get_service_scope();
    let unit_preview = render_unit_file(&ctx.settings, &root, &binary, &scope);
    println!();
    print_info("Будет создан unit-файл:");
    println!("{unit_preview}");

    if !confirm_prompt("Установить службу systemd?", false) {
        print_info("Отменено");
        return Ok(());
    }

    let start_after = confirm_prompt("Запустить службу сразу после установки?", true);
    let (ok, message) = install_service(&ctx.settings, &root, start_after);
    if ok {
        print_success(&message);
        let flag = if message.contains("user") {
            "--user "
        } else {
            ""
        };
        print_info(&format!("Проверка: systemctl {flag}status middlewarejson"));
    } else {
        print_error(&message);
    }
    Ok(())
}

pub fn service_start() -> Result<()> {
    let (ok, message) = start_service(None);
    if ok {
        print_success(&message);
    } else {
        print_error(&message);
        anyhow::bail!(message);
    }
    Ok(())
}

pub fn service_restart() -> Result<()> {
    let (ok, message) = restart_service(None);
    if ok {
        print_success(&message);
    } else {
        print_error(&message);
        anyhow::bail!(message);
    }
    Ok(())
}