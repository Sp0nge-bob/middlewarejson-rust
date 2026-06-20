use serde_json::Value;

use crate::fingerprint::{compute_inbound_fingerprint, extract_transport_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundDescriptor {
    pub fingerprint: String,
    pub remarks: String,
    pub protocol: String,
    pub network: String,
    pub address: String,
    pub path: String,
    pub port: i64,
    pub security: String,
    pub source_index: usize,
    pub panel_inbound_id: i64,
    pub endpoint_index: i64,
}

impl InboundDescriptor {
    pub fn from_config(index: usize, config: &Value) -> Option<Self> {
        let obj = config.as_object()?;
        let remarks = obj
            .get("remarks")
            .map(value_to_string)
            .unwrap_or_else(|| format!("node-{index}"));
        let outbounds = obj.get("outbounds")?.as_array()?;

        let proxy = outbounds.iter().find_map(|outbound| {
            let outbound_obj = outbound.as_object()?;
            let tag = outbound_obj
                .get("tag")
                .map(value_to_string)
                .unwrap_or_default();
            let protocol = outbound_obj
                .get("protocol")
                .map(value_to_string)
                .unwrap_or_default();
            if tag == "proxy"
                || !matches!(protocol.as_str(), "freedom" | "blackhole" | "dns" | "")
            {
                Some(outbound)
            } else {
                None
            }
        })?;

        let stream = proxy
            .get("streamSettings")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let settings = proxy
            .get("settings")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let port = settings
            .get("port")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                settings
                    .get("port")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);

        Some(Self {
            fingerprint: compute_inbound_fingerprint(proxy),
            remarks,
            protocol: proxy
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
            source_index: index,
            panel_inbound_id: 0,
            endpoint_index: 0,
        })
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}