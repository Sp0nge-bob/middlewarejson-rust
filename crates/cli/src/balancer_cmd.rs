use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Table};
use middlewarejson_core::country_flags::{
    apply_flag_prefix, country_code_to_flag, extract_country_code, resolve_flag_choice,
    strip_leading_flag, FlagChoice, COMMON_COUNTRY_FLAGS, CUSTOM_FLAG_MENU_KEY,
};
use middlewarejson_core::db::{BalancerRecord, CatalogRepository, ClientRecord};
use middlewarejson_core::models::balancer::{
    format_scope, format_strategy, normalize_scope, normalize_strategy, strategy_hints,
    BALANCER_SCOPES, BALANCER_STRATEGIES,
};
use middlewarejson_core::services::profile_builder::default_balancer_tag;

use crate::catalog_cmd::catalog_list;
use crate::ctx::AppCtx;
use crate::ui::{
    confirm_prompt, is_exit_choice, print_cancelled, print_error, print_field, print_info,
    print_menu_item, print_section, print_step, print_success, print_warning, prompt_line, CANCEL_HINT,
};

pub fn resolve_member_fingerprints(
    repo: &CatalogRepository,
    members: &[String],
) -> Result<Vec<String>> {
    let mut panel_ids = Vec::new();
    let mut fingerprints = Vec::new();

    for member in members {
        if member.contains('|') {
            fingerprints.push(member.clone());
            continue;
        }
        if let Ok(id) = member.parse::<i64>() {
            panel_ids.push(id);
        } else {
            fingerprints.push(member.clone());
        }
    }

    if !panel_ids.is_empty() {
        let resolved = repo.get_fingerprints_by_panel_ids(&panel_ids, true)?;
        if resolved.is_empty() {
            print_warning(&format!("No active catalog rows for panel IDs: {panel_ids:?}"));
        }
        fingerprints.extend(resolved);
    }

    let mut seen = std::collections::HashSet::new();
    Ok(fingerprints
        .into_iter()
        .filter(|fp| seen.insert(fp.clone()))
        .collect())
}

pub fn print_balancer_table(balancers: &[BalancerRecord]) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "Идентификатор",
        "Название",
        "Стратегия",
        "Область применения",
        "Инбаундов",
    ]);
    for (index, balancer) in balancers.iter().enumerate() {
        table.add_row(vec![
            index.to_string(),
            balancer.tag.clone(),
            balancer.remarks.clone(),
            format_strategy(&balancer.strategy),
            format_scope(&balancer.scope, &balancer.scope_target),
            balancer.member_fingerprints.len().to_string(),
        ]);
    }
    println!("{table}");
}

fn prompt_strategy(default: &str) -> String {
    println!("Стратегия балансировки");
    let hints = strategy_hints();
    for (index, strategy) in BALANCER_STRATEGIES.iter().enumerate() {
        let label = format_strategy(strategy);
        let hint = hints.get(strategy).copied().unwrap_or("");
        let suffix = if hint.is_empty() {
            String::new()
        } else {
            format!(" — {hint}")
        };
        let mark = if *strategy == default {
            " (по умолчанию)"
        } else {
            ""
        };
        println!("  {}. {label}{suffix}{mark}", index + 1);
    }
    let choice = prompt_line(&format!("Выбор [1]"));
    let choice = if choice.is_empty() { "1".to_string() } else { choice };
    if let Ok(index) = choice.parse::<usize>() {
        if index >= 1 && index <= BALANCER_STRATEGIES.len() {
            return BALANCER_STRATEGIES[index - 1].to_string();
        }
    }
    normalize_strategy(&choice).unwrap_or(default).to_string()
}

fn prompt_scope(repo: &CatalogRepository) -> Result<Option<(String, String)>> {
    println!("Область применения");
    for (index, (_scope, label)) in BALANCER_SCOPES.iter().enumerate() {
        println!("  {}. {label}", index + 1);
    }
    let choice = prompt_line(&format!("Выбор, {CANCEL_HINT}"));
    if is_exit_choice(&choice) {
        print_cancelled();
        return Ok(None);
    }
    let scope = if let Ok(index) = choice.parse::<usize>() {
        if index >= 1 && index <= BALANCER_SCOPES.len() {
            BALANCER_SCOPES[index - 1].0.to_string()
        } else {
            "disabled".to_string()
        }
    } else {
        normalize_scope(&choice)
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "disabled".to_string())
    };

    if scope == "group" {
        let groups = repo.list_groups()?;
        if groups.is_empty() {
            print_warning("Групп нет. Сначала выполните синхронизацию.");
            return Ok(Some(("disabled".to_string(), String::new())));
        }
        println!("Выберите группу");
        for (index, group_name) in groups.iter().enumerate() {
            println!("  {index}. {group_name}");
        }
        let group_choice = prompt_line(&format!("Номер группы, {CANCEL_HINT}"));
        if is_exit_choice(&group_choice) {
            print_cancelled();
            return Ok(None);
        }
        if let Ok(index) = group_choice.parse::<usize>() {
            if let Some(group) = groups.get(index) {
                return Ok(Some(("group".to_string(), group.clone())));
            }
        }
        let group_name = prompt_line(&format!("Имя группы, {CANCEL_HINT}"));
        if is_exit_choice(&group_name) {
            print_cancelled();
            return Ok(None);
        }
        return Ok(Some(("group".to_string(), group_name)));
    }

    if scope == "client" {
        let clients = repo.list_all_clients(false)?;
        if clients.is_empty() {
            print_warning("Клиентов нет. Сначала выполните синхронизацию.");
            return Ok(Some(("disabled".to_string(), String::new())));
        }
        let sub_id = prompt_client_sub_id(&clients)?;
        return Ok(sub_id.map(|id| ("client".to_string(), id)));
    }

    Ok(Some((scope, String::new())))
}

fn client_display_label(client: &ClientRecord) -> String {
    let label = if client.email.is_empty() {
        client.sub_id.clone()
    } else {
        client.email.clone()
    };
    let group_hint = if client.group_name.is_empty() {
        String::new()
    } else {
        format!(" [{}]", client.group_name)
    };
    format!("{label}{group_hint}  sub_id={}", client.sub_id)
}

fn prompt_client_sub_id(clients: &[ClientRecord]) -> Result<Option<String>> {
    print_info(&format!(
        "Клиентов в базе: {}. Введите часть email/sub_id или sub_id целиком.",
        clients.len()
    ));
    loop {
        let query = prompt_line(&format!("Поиск, {CANCEL_HINT}"));
        if is_exit_choice(&query) {
            print_cancelled();
            return Ok(None);
        }
        if let Some(client) = clients.iter().find(|c| c.sub_id == query) {
            println!("  {}", client_display_label(client));
            if confirm_prompt("Выбрать этого клиента?", true) {
                return Ok(Some(client.sub_id.clone()));
            }
            continue;
        }
        if query.len() < 2 {
            print_warning("Слишком короткий запрос");
            continue;
        }
        let needle = query.to_lowercase();
        let matches: Vec<_> = clients
            .iter()
            .filter(|c| {
                c.email.to_lowercase().contains(&needle)
                    || c.sub_id.to_lowercase().contains(&needle)
                    || c.group_name.to_lowercase() == needle
            })
            .collect();
        if matches.is_empty() {
            print_warning("Ничего не найдено");
            continue;
        }
        if matches.len() == 1 {
            println!("  {}", client_display_label(matches[0]));
            if confirm_prompt("Выбрать этого клиента?", true) {
                return Ok(Some(matches[0].sub_id.clone()));
            }
            continue;
        }
        let shown = &matches[..matches.len().min(20)];
        println!("Выберите клиента");
        for (index, client) in shown.iter().enumerate() {
            println!("  {index}. {}", client_display_label(client));
        }
        let choice = prompt_line(&format!("Номер клиента, {CANCEL_HINT}"));
        if is_exit_choice(&choice) {
            print_cancelled();
            return Ok(None);
        }
        if let Ok(index) = choice.parse::<usize>() {
            if let Some(client) = shown.get(index) {
                return Ok(Some(client.sub_id.clone()));
            }
        }
        print_error("Неверный номер");
    }
}

fn prompt_happ_remarks(current: Option<&str>) -> Result<Option<String>> {
    let default_name = strip_leading_flag(current.unwrap_or("Balance"));
    let default_flag_code = current.and_then(extract_country_code);

    println!("Название в HAPP");
    print_info("Флаг в начале названия отображается в HAPP как иконка профиля");
    println!("  0. Без флага");
    for (index, (code, label)) in COMMON_COUNTRY_FLAGS.iter().enumerate() {
        let flag = country_code_to_flag(code).unwrap_or_default();
        let mark = if default_flag_code.as_deref() == Some(*code) {
            " (текущий)"
        } else {
            ""
        };
        println!("  {}. {flag} {code} — {label}{mark}", index + 1);
    }
    println!(
        "  {CUSTOM_FLAG_MENU_KEY}. Другой код (любые 2 буквы ISO: ch, se, kz, br…)"
    );

    let mut country_code: Option<String> = None;
    loop {
        let flag_choice = prompt_line(&format!(
            "Флаг (номер, код ch/se или {CUSTOM_FLAG_MENU_KEY} — другой), {CANCEL_HINT}"
        ));
        if is_exit_choice(&flag_choice) {
            print_cancelled();
            return Ok(None);
        }
        match resolve_flag_choice(&flag_choice) {
            FlagChoice::Invalid => {
                print_warning("Неверный выбор — номер, 2-буквенный код или +");
                continue;
            }
            FlagChoice::NoFlag => {
                country_code = None;
                break;
            }
            FlagChoice::CountryCode(code) => {
                country_code = Some(code);
                break;
            }
            FlagChoice::CustomPrompt => loop {
                let custom_code = prompt_line(&format!("Код страны (2 буквы), {CANCEL_HINT}"));
                if is_exit_choice(&custom_code) {
                    print_cancelled();
                    return Ok(None);
                }
                if let FlagChoice::CountryCode(code) = resolve_flag_choice(&custom_code) {
                    let preview = country_code_to_flag(&code).unwrap_or_default();
                    print_info(&format!("Выбран флаг: {preview} {code}"));
                    country_code = Some(code);
                    break;
                }
                print_warning("Нужен код из 2 латинских букв (ISO 3166-1)");
            },
        }
    }

    let name = prompt_line(&format!("Название [{default_name}]"));
    if is_exit_choice(&name) {
        print_cancelled();
        return Ok(None);
    }
    let name = if name.is_empty() { default_name } else { name };
    if name.is_empty() {
        print_warning("Название не может быть пустым");
        return Ok(None);
    }
    let remarks = apply_flag_prefix(&name, country_code.as_deref());
    print_info(&format!("В HAPP: {remarks}"));
    Ok(Some(remarks))
}

fn prompt_member_indices(
    rows: &[serde_json::Map<String, serde_json::Value>],
) -> Result<Option<Vec<String>>> {
    if rows.is_empty() {
        print_warning("Каталог инбаундов пуст. Сначала выполните синхронизацию.");
        return Ok(None);
    }
    let selection = prompt_line(&format!(
        "Номера из колонки # (например 0,2,5), {CANCEL_HINT}"
    ));
    if is_exit_choice(&selection) {
        print_cancelled();
        return Ok(None);
    }
    let indices: Result<Vec<usize>, _> = selection
        .split(',')
        .map(|part| part.trim().parse())
        .collect();
    match indices {
        Ok(indices) => {
            let mut fps = Vec::new();
            for index in indices {
                let fp = rows
                    .get(index)
                    .and_then(|r| r.get("fingerprint"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("invalid index"))?;
                fps.push(fp.to_string());
            }
            Ok(Some(fps))
        }
        Err(_) => {
            print_error("Неверные номера строк");
            Ok(None)
        }
    }
}

pub fn balancer_list(ctx: &AppCtx) -> Result<()> {
    let repo = ctx.repo()?;
    let balancers = repo.list_balancers()?;
    if balancers.is_empty() {
        print_warning("Балансировщиков нет");
        return Ok(());
    }
    print_balancer_table(&balancers);
    Ok(())
}

pub fn balancer_create(
    ctx: &AppCtx,
    name: &str,
    flag: Option<&str>,
    members: &[String],
    tag: Option<&str>,
    strategy: &str,
    scope: &str,
    scope_target: &str,
) -> Result<()> {
    let repo = ctx.repo()?;
    let fingerprints = resolve_member_fingerprints(&repo, members)?;
    if fingerprints.is_empty() {
        anyhow::bail!("--members must resolve to at least one fingerprint");
    }
    let remarks = apply_flag_prefix(name, flag);
    let balancer_tag = tag
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_balancer_tag(&remarks));
    repo.create_balancer(
        &balancer_tag,
        &remarks,
        strategy,
        &fingerprints,
        scope,
        scope_target,
    )?;
    print_success(&format!(
        "Балансировщик «{balancer_tag}» создан — {}, {}",
        format_scope(scope, scope_target),
        format_strategy(strategy)
    ));
    Ok(())
}

fn create_balancer_interactive(ctx: &AppCtx) -> Result<()> {
    print_step(1, 4, "Состав инбаундов");
    let rows = catalog_list(ctx, true, true)?;
    let Some(fingerprints) = prompt_member_indices(&rows)? else {
        return Ok(());
    };

    print_step(2, 4, "Название в HAPP");
    let Some(name) = prompt_happ_remarks(Some("Balance"))? else {
        return Ok(());
    };
    let tag = default_balancer_tag(&name);
    print_info(&format!("Идентификатор будет: {tag}"));

    print_step(3, 4, "Стратегия балансировки");
    let strategy = prompt_strategy("roundRobin");

    print_step(4, 4, "Область применения");
    let repo = ctx.repo()?;
    let Some((scope, scope_target)) = prompt_scope(&repo)? else {
        return Ok(());
    };

    repo.create_balancer(&tag, &name, &strategy, &fingerprints, &scope, &scope_target)?;
    print_success(&format!(
        "Балансировщик «{tag}» создан — {}, {}, {} инбаундов",
        format_scope(&scope, &scope_target),
        format_strategy(&strategy),
        fingerprints.len()
    ));
    Ok(())
}

fn resolve_balancer_tag(
    repo: &CatalogRepository,
    balancers: &[BalancerRecord],
    choice: &str,
) -> Result<Option<String>> {
    let value = choice.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if let Ok(index) = value.parse::<usize>() {
        return Ok(balancers.get(index).map(|b| b.tag.clone()));
    }
    if repo.get_balancer_by_tag(value)?.is_some() {
        return Ok(Some(value.to_string()));
    }
    print_error(&format!("Балансировщик «{value}» не найден"));
    Ok(None)
}

pub fn configure_balancer_interactive(ctx: &AppCtx, balancer_tag: &str) -> Result<()> {
    let repo = ctx.repo()?;
    let Some(mut balancer) = repo.get_balancer_by_tag(balancer_tag)? else {
        print_error(&format!("Балансировщик «{balancer_tag}» не найден"));
        return Ok(());
    };

    loop {
        println!();
        println!("{}", balancer.remarks);
        print_field("Идентификатор", &balancer.tag);
        print_field("Стратегия балансировки", &format_strategy(&balancer.strategy));
        print_field(
            "Область применения",
            &format_scope(&balancer.scope, &balancer.scope_target),
        );
        print_field("Состав инбаундов", &format!("{} шт.", balancer.member_fingerprints.len()));
        println!("  1. Область применения");
        println!("  2. Стратегия балансировки");
        println!("  3. Название в HAPP");
        println!("  4. Состав инбаундов");
        println!("  0. Назад");

        let choice = prompt_line("Выбор [0]");
        if choice.is_empty() || choice == "0" {
            return Ok(());
        }
        match choice.as_str() {
            "1" => {
                if let Some((scope, target)) = prompt_scope(&repo)? {
                    repo.set_balancer_scope(&balancer.tag, &scope, &target)?;
                    print_success(&format!("Область применения: {}", format_scope(&scope, &target)));
                }
            }
            "2" => {
                let strategy = prompt_strategy(&balancer.strategy);
                repo.update_balancer(&balancer.tag, None, Some(&strategy), None, None, None)?;
                print_success(&format!("Стратегия: {}", format_strategy(&strategy)));
            }
            "3" => {
                if let Some(remarks) = prompt_happ_remarks(Some(&balancer.remarks))? {
                    repo.update_balancer(&balancer.tag, Some(&remarks), None, None, None, None)?;
                    print_success(&format!("Название: {remarks}"));
                }
            }
            "4" => {
                let rows = catalog_list(ctx, true, true)?;
                if let Some(fingerprints) = prompt_member_indices(&rows)? {
                    repo.update_balancer(&balancer.tag, None, None, None, None, Some(&fingerprints))?;
                    print_success(&format!("Состав обновлён ({} инбаундов)", fingerprints.len()));
                }
            }
            _ => print_warning("Неизвестный пункт"),
        }
        if let Some(updated) = repo.get_balancer_by_tag(balancer_tag)? {
            balancer = updated;
        }
    }
}

fn delete_balancer_interactive(ctx: &AppCtx, balancers: &[BalancerRecord]) -> Result<()> {
    if balancers.is_empty() {
        print_warning("Балансировщиков нет");
        return Ok(());
    }
    let choice = prompt_line(&format!(
        "Номер или идентификатор для удаления (Enter — отмена, {CANCEL_HINT})"
    ));
    if is_exit_choice(&choice) {
        print_cancelled();
        return Ok(());
    }
    let repo = ctx.repo()?;
    let Some(tag) = resolve_balancer_tag(&repo, balancers, &choice)? else {
        return Ok(());
    };
    if !confirm_prompt(&format!("Удалить балансировщик «{tag}»?"), false) {
        return Ok(());
    }
    if repo.delete_balancer(&tag)? {
        print_success(&format!("Удалён балансировщик «{tag}»"));
    } else {
        print_error(&format!("Балансировщик «{tag}» не найден"));
    }
    Ok(())
}

pub fn run_balancers_menu(ctx: &AppCtx) -> Result<()> {
    loop {
        println!();
        print_section("Балансировщики");
        let repo = ctx.repo()?;
        let balancers = repo.list_balancers()?;
        if balancers.is_empty() {
            print_warning("Балансировщиков нет");
        } else {
            print_balancer_table(&balancers);
        }

        println!();
        print_menu_item(1, "Создать");
        print_menu_item(2, "Настроить");
        print_menu_item(3, "Удалить");
        print_menu_item(0, "Назад");

        let choice = prompt_line("Выбор [0 — назад]");
        if choice.is_empty() || choice == "0" {
            return Ok(());
        }
        match choice.as_str() {
            "1" => create_balancer_interactive(ctx)?,
            "2" => {
                if balancers.is_empty() {
                    print_warning("Сначала создайте балансировщик");
                    continue;
                }
                let pick = prompt_line(&format!(
                    "Номер или идентификатор (Enter — отмена, {CANCEL_HINT})"
                ));
                if is_exit_choice(&pick) {
                    print_cancelled();
                    continue;
                }
                if let Some(tag) = resolve_balancer_tag(&repo, &balancers, &pick)? {
                    configure_balancer_interactive(ctx, &tag)?;
                }
            }
            "3" => delete_balancer_interactive(ctx, &balancers)?,
            _ => print_warning("Неизвестный пункт"),
        }
    }
}