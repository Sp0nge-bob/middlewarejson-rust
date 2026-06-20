use std::collections::HashMap;

use thiserror::Error;

pub type BalancerScope = &'static str;
pub type BalancerStrategy = &'static str;

pub const BALANCER_SCOPES: &[(&str, &str)] = &[
    ("disabled", "Выключен"),
    ("group", "Группа"),
    ("all", "Все клиенты"),
    ("client", "Один клиент"),
];

pub const BALANCER_STRATEGIES: &[&str] =
    &["roundRobin", "leastLoad", "leastPing", "random"];

pub fn strategy_hints() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("roundRobin", "равномерно по очереди"),
        ("leastLoad", "на узел с меньшей нагрузкой"),
        ("leastPing", "на узел с минимальным пингом"),
        ("random", "случайный выбор"),
    ])
}

pub fn strategy_labels() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("roundRobin", "По очереди"),
        ("leastLoad", "Меньше нагрузка"),
        ("leastPing", "Минимальный пинг"),
        ("random", "Случайный выбор"),
    ])
}

pub fn scope_priority() -> HashMap<&'static str, i32> {
    HashMap::from([
        ("client", 3),
        ("group", 2),
        ("all", 1),
        ("disabled", 0),
    ])
}

pub fn format_strategy(strategy: &str) -> String {
    strategy_labels()
        .get(strategy)
        .copied()
        .unwrap_or(strategy)
        .to_string()
}

pub fn format_scope(scope: &str, scope_target: &str) -> String {
    let label = BALANCER_SCOPES
        .iter()
        .find(|(k, _)| *k == scope)
        .map(|(_, v)| *v)
        .unwrap_or(scope);
    if (scope == "group" || scope == "client") && !scope_target.is_empty() {
        format!("{label}: {scope_target}")
    } else {
        label.to_string()
    }
}

#[derive(Debug, Error)]
pub enum BalancerNormalizeError {
    #[error("unknown scope: {0}")]
    UnknownScope(String),
    #[error("unknown strategy: {0}")]
    UnknownStrategy(String),
}

pub fn normalize_scope(scope: &str) -> Result<BalancerScope, BalancerNormalizeError> {
    let value = scope.trim().to_lowercase();
    if BALANCER_SCOPES.iter().any(|(k, _)| *k == value) {
        Ok(BALANCER_SCOPES
            .iter()
            .find(|(k, _)| *k == value)
            .map(|(k, _)| *k)
            .unwrap())
    } else {
        Err(BalancerNormalizeError::UnknownScope(scope.to_string()))
    }
}

pub fn normalize_strategy(strategy: &str) -> Result<BalancerStrategy, BalancerNormalizeError> {
    let value = strategy.trim();
    if BALANCER_STRATEGIES.contains(&value) {
        Ok(BALANCER_STRATEGIES
            .iter()
            .find(|s| **s == value)
            .copied()
            .unwrap())
    } else {
        Err(BalancerNormalizeError::UnknownStrategy(strategy.to_string()))
    }
}