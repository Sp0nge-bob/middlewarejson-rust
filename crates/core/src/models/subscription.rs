use regex::Regex;
use serde_json::Value;
use thiserror::Error;
use url::Url;

use std::sync::LazyLock;

static SUB_ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{8,64}$").expect("valid sub_id regex"));

pub type SubscriptionPayload = Value;

#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("{0}")]
    Message(String),
}

pub fn validate_sub_id(sub_id: &str) -> bool {
    SUB_ID_PATTERN.is_match(sub_id)
}

pub fn parse_subscription_reference(value: &str) -> Result<String, SubscriptionError> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err(SubscriptionError::Message("Пустое значение".to_string()));
    }

    if validate_sub_id(raw) {
        return Ok(raw.to_string());
    }

    let parsed = match Url::parse(raw) {
        Ok(url) if !url.scheme().is_empty() || url.host().is_some() => url,
        _ => {
            return Err(SubscriptionError::Message(
                "Укажите ссылку на JSON-подписку, например \
                 https://example.com/<json-path>/<sub_id>"
                    .to_string(),
            ));
        }
    };

    let path = parsed.path().trim_end_matches('/');
    let candidate = path.rsplit('/').next().unwrap_or("");
    if validate_sub_id(candidate) {
        return Ok(candidate.to_string());
    }

    Err(SubscriptionError::Message(
        "Не удалось извлечь sub_id. Пример: \
         https://example.com/<json-path>/<sub_id>"
            .to_string(),
    ))
}

pub fn validate_payload(payload: &SubscriptionPayload) -> Result<(), SubscriptionError> {
    let configs: Vec<&Value> = match payload {
        Value::Array(items) => items.iter().collect(),
        obj @ Value::Object(_) => vec![obj],
        _ => {
            return Err(SubscriptionError::Message(
                "empty subscription payload".to_string(),
            ));
        }
    };

    if configs.is_empty() {
        return Err(SubscriptionError::Message(
            "empty subscription payload".to_string(),
        ));
    }

    for config in configs {
        let Value::Object(config_obj) = config else {
            return Err(SubscriptionError::Message(
                "subscription item must be an object".to_string(),
            ));
        };
        let outbounds = config_obj.get("outbounds");
        if !outbounds.map(|v| v.is_array()).unwrap_or(false) {
            return Err(SubscriptionError::Message(
                "missing or invalid outbounds array".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_sub_id_accepts_alphanumeric() {
        assert!(validate_sub_id("client_a_sub_id12"));
    }

    #[test]
    fn parse_subscription_reference_from_url() {
        let sub_id = parse_subscription_reference(
            "https://example.com/json/client_a_sub_id12",
        )
        .unwrap();
        assert_eq!(sub_id, "client_a_sub_id12");
    }
}