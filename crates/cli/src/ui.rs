use std::io::{self, Write};

pub const CANCEL_HINT: &str = "exit — отмена";

pub fn is_exit_choice(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "exit" | "выход" | "quit" | "q" | "отмена" | "cancel"
    )
}

pub fn print_header(title: &str, subtitle: &str) {
    println!();
    println!("=== {title} ===");
    if !subtitle.is_empty() {
        println!("{subtitle}");
    }
}

pub fn print_section(title: &str) {
    println!();
    println!("{title}");
}

pub fn print_menu_item(number: impl std::fmt::Display, text: &str) {
    println!("  {number}. {text}");
}

pub fn print_step(step: u32, total: u32, title: &str) {
    println!();
    println!("Шаг {step}/{total}. {title}");
}

pub fn print_success(message: &str) {
    println!("✓ {message}");
}

pub fn print_error(message: &str) {
    eprintln!("✗ {message}");
}

pub fn print_warning(message: &str) {
    println!("! {message}");
}

pub fn print_info(message: &str) {
    println!("  {message}");
}

pub fn print_field(label: &str, value: &str) {
    println!("  {label}: {value}");
}

pub fn print_cancelled() {
    print_info("Отменено");
}

pub fn prompt_line(label: &str) -> String {
    print!("{label}: ");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    if io::stdin().read_line(&mut buf).is_err() {
        return String::new();
    }
    buf.trim().to_string()
}

pub fn confirm_prompt(message: &str, default: bool) -> bool {
    let hint = if default { "(Y/n)" } else { "(y/N)" };
    let value = prompt_line(&format!("{message} {hint}"));
    if value.is_empty() {
        return default;
    }
    matches!(
        value.to_lowercase().as_str(),
        "y" | "yes" | "д" | "да"
    )
}

pub fn mask_token(token: &str) -> String {
    if token.is_empty() {
        return "(not set)".to_string();
    }
    if token.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &token[..4], &token[token.len() - 4..])
}