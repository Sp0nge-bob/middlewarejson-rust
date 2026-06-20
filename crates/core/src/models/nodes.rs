use serde_json::Value;

use crate::fingerprint::compute_inbound_fingerprint;

const PROXY_TAGS: &[&str] = &["proxy"];

#[derive(Debug, Clone)]
pub struct ProxyNode {
    pub remarks: String,
    pub protocol: String,
    pub outbound: Value,
    pub source_index: usize,
    pub tag: String,
    pub network: String,
    pub fingerprint: String,
}

impl ProxyNode {
    pub fn with_tag(&self, tag: &str) -> Self {
        let mut outbound = self.outbound.clone();
        if let Value::Object(ref mut map) = outbound {
            map.insert("tag".to_string(), Value::String(tag.to_string()));
        }
        Self {
            remarks: self.remarks.clone(),
            protocol: self.protocol.clone(),
            outbound,
            source_index: self.source_index,
            tag: tag.to_string(),
            network: self.network.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

pub fn extract_network(outbound: &Value) -> String {
    match outbound.get("streamSettings") {
        Some(Value::Object(stream)) => stream
            .get("network")
            .map(value_to_string)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn is_proxy_outbound(outbound: &Value) -> bool {
    let Some(obj) = outbound.as_object() else {
        return false;
    };
    let tag = obj.get("tag").map(value_to_string).unwrap_or_default();
    if PROXY_TAGS.contains(&tag.as_str()) {
        return true;
    }
    let protocol = obj
        .get("protocol")
        .map(value_to_string)
        .unwrap_or_default();
    !matches!(protocol.as_str(), "freedom" | "blackhole" | "dns")
}

pub fn configs_to_nodes(configs: &[Value]) -> Vec<ProxyNode> {
    let mut nodes = Vec::new();
    for (index, config) in configs.iter().enumerate() {
        let Some(obj) = config.as_object() else {
            continue;
        };
        let remarks = obj
            .get("remarks")
            .map(value_to_string)
            .unwrap_or_else(|| format!("node-{index}"));
        let Some(outbounds) = obj.get("outbounds").and_then(|v| v.as_array()) else {
            continue;
        };
        for outbound in outbounds {
            if !is_proxy_outbound(outbound) {
                continue;
            }
            nodes.push(ProxyNode {
                remarks: remarks.clone(),
                protocol: outbound
                    .get("protocol")
                    .map(value_to_string)
                    .unwrap_or_default(),
                outbound: outbound.clone(),
                source_index: index,
                tag: String::new(),
                network: extract_network(outbound),
                fingerprint: compute_inbound_fingerprint(outbound),
            });
        }
    }
    nodes
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}