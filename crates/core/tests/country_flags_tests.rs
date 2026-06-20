use middlewarejson_core::country_flags::{
    apply_flag_prefix, country_code_to_flag, extract_country_code, resolve_flag_choice,
    strip_leading_flag, FlagChoice,
};

#[test]
fn country_code_to_flag_nl() {
    assert_eq!(country_code_to_flag("nl"), Some("🇳🇱".to_string()));
}

#[test]
fn apply_and_strip_flag() {
    let with_flag = apply_flag_prefix("Balance", Some("US"));
    assert!(with_flag.starts_with("🇺🇸"));
    assert_eq!(strip_leading_flag(&with_flag), "Balance");
    assert_eq!(extract_country_code(&with_flag), Some("US".to_string()));
}

#[test]
fn resolve_flag_choice_variants() {
    assert_eq!(resolve_flag_choice("0"), FlagChoice::NoFlag);
    assert_eq!(resolve_flag_choice("1"), FlagChoice::CountryCode("NL".to_string()));
    assert_eq!(resolve_flag_choice("ch"), FlagChoice::CountryCode("CH".to_string()));
    assert!(matches!(resolve_flag_choice("+"), FlagChoice::CustomPrompt));
    assert_eq!(resolve_flag_choice("zzz"), FlagChoice::Invalid);
}