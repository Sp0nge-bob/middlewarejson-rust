use std::fs;
use std::path::PathBuf;

use middlewarejson_core::fingerprint::compute_inbound_fingerprint;
use middlewarejson_core::models::inbound::InboundDescriptor;
use middlewarejson_core::models::nodes::configs_to_nodes;
use serde_json::Value;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn load_fixture(name: &str) -> Value {
    let content = fs::read_to_string(fixture_path(name)).expect("read fixture");
    serde_json::from_str(&content).expect("parse fixture")
}

#[test]
fn all_inbounds_have_unique_fingerprints() {
    let configs = load_fixture("raw_3xui_subscription.json");
    let configs = configs.as_array().expect("array fixture");
    let fingerprints: Vec<String> = configs
        .iter()
        .enumerate()
        .filter_map(|(index, config)| {
            InboundDescriptor::from_config(index, config).map(|descriptor| descriptor.fingerprint)
        })
        .collect();
    assert_eq!(fingerprints.len(), 10);
    assert_eq!(fingerprints.iter().collect::<std::collections::HashSet<_>>().len(), 10);
}

#[test]
fn fingerprint_format_for_nl_ws() {
    let configs = load_fixture("raw_3xui_subscription.json");
    let configs = configs.as_array().expect("array fixture");
    let outbound = &configs[0]["outbounds"][0];
    assert_eq!(
        compute_inbound_fingerprint(outbound),
        "vless|node1.example.com|ws|/ws-path|443|tls|"
    );
}

#[test]
fn nodes_receive_fingerprint() {
    let configs = load_fixture("raw_3xui_subscription.json");
    let configs = configs.as_array().expect("array fixture");
    let nodes = configs_to_nodes(configs);
    assert_eq!(nodes.len(), 10);
    assert!(nodes.iter().all(|node| !node.fingerprint.is_empty()));
    assert_eq!(
        nodes
            .iter()
            .map(|node| &node.fingerprint)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        10
    );
}