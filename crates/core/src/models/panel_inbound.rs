use serde_json::{Map, Value};

use crate::fingerprint::{compute_inbound_fingerprint, extract_transport_path};
use crate::models::inbound::InboundDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelEndpoint {
    pub address: String,
    pub port: i64,
    pub force_tls: String,
    pub endpoint_index: usize,
}

pub fn parse_json_field(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::String(s)) if !s.trim().is_empty() => {
            serde_json::from_str(s).ok().and_then(|v: Value| v.as_object().cloned()).unwrap_or_default()
        }
        _ => Map::new(),
    }
}

fn resolve_default_address(inbound: &Map<String, Value>) -> String {
    let listen = inbound
        .get("listen")
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string();
    if !listen.is_empty() && listen != "0.0.0.0" && listen != "::" {
        return listen;
    }

    let share_addr = inbound
        .get("shareAddr")
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string();
    if !share_addr.is_empty() {
        return share_addr;
    }

    String::new()
}

pub fn expand_panel_endpoints(inbound: &Map<String, Value>) -> Vec<PanelEndpoint> {
    let stream = parse_json_field(inbound.get("streamSettings"));
    let external_proxies = stream.get("externalProxy");

    let default_port = inbound
        .get("port")
        .and_then(|v| v.as_i64())
        .or_else(|| inbound.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
        .unwrap_or(0);
    let default_dest = resolve_default_address(inbound);

    if let Some(Value::Array(items)) = external_proxies {
        let mut endpoints = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let Some(item_obj) = item.as_object() else {
                continue;
            };
            let dest = item_obj
                .get("dest")
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            let dest = if dest.is_empty() {
                default_dest.clone()
            } else {
                dest
            };
            let port = item_obj
                .get("port")
                .and_then(|v| v.as_i64())
                .or_else(|| item_obj.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
                .unwrap_or(default_port);
            let force_tls = item_obj
                .get("forceTls")
                .map(value_to_string)
                .unwrap_or_else(|| "same".to_string());
            endpoints.push(PanelEndpoint {
                address: dest,
                port,
                force_tls,
                endpoint_index: index,
            });
        }
        if !endpoints.is_empty() {
            return endpoints;
        }
    }

    vec![PanelEndpoint {
        address: default_dest,
        port: default_port,
        force_tls: "same".to_string(),
        endpoint_index: 0,
    }]
}

fn apply_force_tls(stream: &mut Map<String, Value>, force_tls: &str) {
    match force_tls {
        "tls" => {
            if stream.get("security").and_then(|v| v.as_str()) != Some("tls") {
                stream.insert("security".to_string(), Value::String("tls".to_string()));
                stream.insert("tlsSettings".to_string(), Value::Object(Map::new()));
            }
        }
        "none" => {
            if stream.get("security").and_then(|v| v.as_str()) != Some("none") {
                stream.insert("security".to_string(), Value::String("none".to_string()));
                stream.remove("tlsSettings");
            }
        }
        _ => {}
    }
}

pub fn endpoint_to_outbound(inbound: &Map<String, Value>, endpoint: &PanelEndpoint) -> Value {
    let protocol = inbound
        .get("protocol")
        .map(value_to_string)
        .unwrap_or_default();
    let mut stream = parse_json_field(inbound.get("streamSettings"));
    stream.remove("externalProxy");
    apply_force_tls(&mut stream, &endpoint.force_tls);

    serde_json::json!({
        "protocol": protocol,
        "tag": "proxy",
        "streamSettings": stream,
        "settings": {
            "address": endpoint.address,
            "port": endpoint.port,
        }
    })
}

pub fn panel_inbound_to_descriptors(inbound: &Map<String, Value>) -> Vec<InboundDescriptor> {
    if inbound.get("enable") == Some(&Value::Bool(false)) {
        return Vec::new();
    }

    let panel_inbound_id = inbound
        .get("id")
        .and_then(|v| v.as_i64())
        .or_else(|| inbound.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
        .unwrap_or(0);
    let remark = inbound
        .get("remark")
        .map(value_to_string)
        .unwrap_or_default();
    let endpoints = expand_panel_endpoints(inbound);
    let mut descriptors = Vec::new();

    for endpoint in endpoints {
        let outbound = endpoint_to_outbound(inbound, &endpoint);
        let stream = outbound
            .get("streamSettings")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let settings = outbound
            .get("settings")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let port = settings
            .get("port")
            .and_then(|v| v.as_i64())
            .or_else(|| settings.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);

        descriptors.push(InboundDescriptor {
            fingerprint: compute_inbound_fingerprint(&outbound),
            remarks: remark.clone(),
            protocol: outbound
                .get("protocol")
                .map(value_to_string)
                .unwrap_or_default(),
            network: stream
                .get("network")
                .map(value_to_string)
                .unwrap_or_default(),
            address: settings
                .get("address")
                .map(value_to_string)
                .unwrap_or_default(),
            path: extract_transport_path(&Value::Object(stream.clone())),
            port,
            security: stream
                .get("security")
                .map(value_to_string)
                .unwrap_or_default(),
            source_index: endpoint.endpoint_index,
            panel_inbound_id,
            endpoint_index: endpoint.endpoint_index as i64,
        });
    }

    descriptors
}

pub fn panel_inbounds_to_descriptors(inbounds: &[Value]) -> Vec<InboundDescriptor> {
    let mut result = Vec::new();
    for inbound in inbounds {
        if let Some(obj) = inbound.as_object() {
            result.extend(panel_inbound_to_descriptors(obj));
        }
    }
    result
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}