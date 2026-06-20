use crate::fingerprint::extract_transport_path;
use crate::models::nodes::ProxyNode;
use crate::models::rules_schema::MatchRule;

pub fn node_matches(node: &ProxyNode, rule: &MatchRule) -> bool {
    if !rule.fingerprint_equals.is_empty()
        && !rule.fingerprint_equals.contains(&node.fingerprint)
    {
        return false;
    }

    if !rule.remarks_equals.is_empty() && !rule.remarks_equals.contains(&node.remarks) {
        return false;
    }

    if !rule.remarks_contains.is_empty()
        && !rule
            .remarks_contains
            .iter()
            .any(|part| node.remarks.contains(part))
    {
        return false;
    }

    if !rule.flag.is_empty() && !node.remarks.contains(&rule.flag) {
        return false;
    }

    if !rule.network_in.is_empty() && !rule.network_in.contains(&node.network) {
        return false;
    }

    if !rule.protocol.is_empty() && node.protocol != rule.protocol {
        return false;
    }

    if !rule.network.is_empty() && node.network != rule.network {
        return false;
    }

    if !rule.address_equals.is_empty() {
        let address = node_address(node);
        if !rule.address_equals.contains(&address) {
            return false;
        }
    }

    if !rule.path_equals.is_empty() {
        let path = node_path(node);
        if !rule.path_equals.contains(&path) {
            return false;
        }
    }

    if !rule.security.is_empty() && node_security(node) != rule.security {
        return false;
    }

    if let Some(port) = rule.port {
        if node_port(node) != port {
            return false;
        }
    }

    true
}

fn node_settings(node: &ProxyNode) -> serde_json::Map<String, serde_json::Value> {
    node.outbound
        .get("settings")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn node_stream(node: &ProxyNode) -> serde_json::Map<String, serde_json::Value> {
    node.outbound
        .get("streamSettings")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn node_address(node: &ProxyNode) -> String {
    node_settings(node)
        .get("address")
        .map(value_to_string)
        .unwrap_or_default()
}

fn node_path(node: &ProxyNode) -> String {
    extract_transport_path(&serde_json::Value::Object(node_stream(node)))
}

fn node_security(node: &ProxyNode) -> String {
    node_stream(node)
        .get("security")
        .map(value_to_string)
        .unwrap_or_default()
}

fn node_port(node: &ProxyNode) -> i64 {
    node_settings(node)
        .get("port")
        .and_then(|v| v.as_i64())
        .or_else(|| node_settings(node).get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        v => v.to_string(),
    }
}