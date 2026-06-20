use regex::Regex;
use std::sync::LazyLock;

pub const CUSTOM_FLAG_MENU_KEY: &str = "+";

pub const COMMON_COUNTRY_FLAGS: &[(&str, &str)] = &[
    ("NL", "Нидерланды"),
    ("US", "США"),
    ("DE", "Германия"),
    ("RU", "Россия"),
    ("GB", "Великобритания"),
    ("FR", "Франция"),
    ("FI", "Финляндия"),
    ("TR", "Турция"),
    ("AE", "ОАЭ"),
    ("SG", "Сингапур"),
    ("JP", "Япония"),
    ("KR", "Корея"),
    ("CA", "Канада"),
    ("PL", "Польша"),
    ("UA", "Украина"),
];

static FLAG_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\u{1F1E6}-\u{1F1FF}]{2}\s*").expect("valid flag regex"));

const REGIONAL_BASE: u32 = 0x1F1E6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagChoice {
    NoFlag,
    CountryCode(String),
    CustomPrompt,
    Invalid,
}

pub fn country_code_to_flag(code: &str) -> Option<String> {
    let value = code.trim().to_uppercase();
    if value.len() != 2 || !value.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let flag: String = value
        .chars()
        .filter_map(|ch| char::from_u32(REGIONAL_BASE + (ch as u32 - b'A' as u32)))
        .collect();
    if flag.chars().count() != 2 {
        return None;
    }
    Some(flag)
}

pub fn strip_leading_flag(text: &str) -> String {
    FLAG_PREFIX_RE.replace(text, "").trim().to_string()
}

pub fn extract_country_code(text: &str) -> Option<String> {
    let matched = FLAG_PREFIX_RE.find(text)?;
    let flag = matched.as_str().trim();
    if flag.chars().count() != 2 {
        return None;
    }
    let mut letters = String::new();
    for ch in flag.chars() {
        let offset = ch as u32;
        if offset < REGIONAL_BASE || offset > REGIONAL_BASE + 25 {
            return None;
        }
        let Some(letter) = char::from_u32(b'A' as u32 + (offset - REGIONAL_BASE)) else {
            return None;
        };
        letters.push(letter);
    }
    Some(letters)
}

pub fn apply_flag_prefix(name: &str, country_code: Option<&str>) -> String {
    let base = strip_leading_flag(name);
    let Some(code) = country_code else {
        return base;
    };
    let Some(flag) = country_code_to_flag(code) else {
        return base;
    };
    if base.is_empty() {
        flag
    } else {
        format!("{flag} {base}")
    }
}

pub fn resolve_flag_choice(choice: &str) -> FlagChoice {
    let value = choice.trim();
    if value.is_empty() || value == "0" {
        return FlagChoice::NoFlag;
    }

    let lower = value.to_lowercase();
    if value == CUSTOM_FLAG_MENU_KEY
        || matches!(lower.as_str(), "другой" | "other" | "*")
    {
        return FlagChoice::CustomPrompt;
    }

    if let Ok(index) = value.parse::<usize>() {
        if index >= 1 && index <= COMMON_COUNTRY_FLAGS.len() {
            return FlagChoice::CountryCode(COMMON_COUNTRY_FLAGS[index - 1].0.to_string());
        }
    }

    if country_code_to_flag(value).is_some() {
        return FlagChoice::CountryCode(value.to_uppercase().chars().take(2).collect());
    }

    FlagChoice::Invalid
}