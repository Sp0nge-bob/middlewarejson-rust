use std::process::{Command, Stdio};

use middlewarejson_core::services::systemd_service::detect_server_binary;

use crate::ctx::AppCtx;
use crate::ui::{print_error, print_info};

pub fn run_server(ctx: &AppCtx) -> anyhow::Result<()> {
    let root = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let binary = detect_server_binary(&root);
    let host = &ctx.settings.agent_host;
    let port = ctx.settings.agent_port;

    if binary.is_file() {
        print_info(&format!(
            "Запуск {path} — Ctrl+C для остановки",
            path = binary.display()
        ));
        let status = Command::new(&binary)
            .current_dir(&root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(s) if s.success() => print_info("Сервер остановлен"),
            Ok(_) => print_info("Сервер завершился с ошибкой"),
            Err(error) => print_error(&format!("Не удалось запустить сервер: {error}")),
        }
        return Ok(());
    }

    print_info(&format!(
        "Release-бинарник не найден. Соберите: cargo build --release\n\
         Или: cargo run -p middlewarejson-server\n\
         Агент: http://{host}:{port}/health"
    ));
    Ok(())
}