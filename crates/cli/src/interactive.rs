use anyhow::Result;

use crate::balancer_cmd::run_balancers_menu;
use crate::catalog_cmd::catalog_list;
use crate::ctx::AppCtx;
use crate::groups_cmd::group_list;
use crate::run_server::run_server;
use crate::service_cmd::{install_systemd, service_status_menu};
use crate::settings_cmd::{
    edit_panel_settings, panel_settings_show, panel_test, script_settings_show,
};
use crate::sync_cmd::sync_all;
use crate::ui::{
    confirm_prompt, print_header, print_menu_item, print_section, print_warning, prompt_line,
};

fn print_interactive_menu(ctx: &AppCtx) {
    print_section("Настройки");
    print_menu_item(1, "Показать настройки панели");
    print_menu_item(2, "Показать настройки скрипта");
    print_menu_item(3, "Проверить подключение к панели");
    print_menu_item(4, "Проверить состояние скрипта (systemd)");
    print_menu_item(5, "Установить службу systemd");

    print_section("Данные панели");
    print_menu_item(6, "Список инбаундов");
    print_menu_item(7, "Список групп");

    print_section("Настройка JSON");
    print_menu_item(8, "Балансировщики");

    print_section("Синхронизация");
    print_menu_item(9, "Синхронизация");

    print_section("Отладка");
    print_menu_item(
        10,
        &format!(
            "Запустить агент вручную (middlewarejson :{})",
            ctx.settings.agent_port
        ),
    );

    println!();
    print_menu_item(0, "Выход");
}

pub async fn run_interactive_menu(ctx: &AppCtx) -> Result<()> {
    print_header(
        "middlewarejson",
        "трансформация JSON-подписок 3x-ui (Rust)",
    );

    loop {
        print_interactive_menu(ctx);
        let choice = prompt_line("Выбор [0]");
        let choice = if choice.is_empty() { "0".to_string() } else { choice };

        match choice.as_str() {
            "0" => {
                println!("До свидания");
                break;
            }
            "1" => {
                panel_settings_show(ctx)?;
                if confirm_prompt("Изменить настройки панели?", false) {
                    edit_panel_settings(ctx)?;
                }
            }
            "2" => script_settings_show(ctx)?,
            "3" => panel_test(ctx).await?,
            "4" => service_status_menu(ctx)?,
            "5" => install_systemd(ctx)?,
            "6" => {
                catalog_list(ctx, false, false)?;
            }
            "7" => group_list(ctx)?,
            "8" => run_balancers_menu(ctx)?,
            "9" => sync_all(ctx).await?,
            "10" => run_server(ctx)?,
            _ => print_warning("Неизвестный пункт"),
        }
    }
    Ok(())
}