use std::fs;
use std::path::PathBuf;

use middlewarejson_core::models::nodes::configs_to_nodes;
use middlewarejson_core::models::rules_schema::TransformRules;
use middlewarejson_core::services::transformer::SubscriptionTransformer;
use middlewarejson_core::transform::rules_engine::RulesTransformer;
use serde_json::{json, Value};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn load_fixture(name: &str) -> Value {
    let content = fs::read_to_string(fixture_path(name)).expect("read fixture");
    serde_json::from_str(&content).expect("parse fixture")
}

fn base_rules() -> Value {
    json!({
        "output": {"format": "grouped"},
        "filters": {"exclude": []},
        "tagging": {
            "rules": [
                {"match": {"remarks_equals": ["NL-WS"]}, "tag": "nl-ws"},
                {"match": {"remarks_equals": ["NL-XHTTP"]}, "tag": "nl-xhttp"},
                {"match": {"remarks_equals": ["NL-GRPC"]}, "tag": "nl-grpc"},
                {"match": {"remarks_equals": ["US-WS"]}, "tag": "us-ws"},
                {"match": {"remarks_equals": ["US-GRPC"]}, "tag": "us-grpc"},
            ],
            "default_template": "node-{index}",
        },
        "balancers": [
            {
                "tag": "global-pool",
                "remarks": "NL+USA Balance",
                "strategy": "roundRobin",
                "members": [{"tags": ["nl-ws", "us-ws"]}],
            }
        ],
    })
}

#[test]
fn global_balancer_nl_and_us_from_different_servers() {
    let payload = load_fixture("sample_nodes.json");
    let rules = TransformRules::from_value(&base_rules()).expect("rules");
    let result = RulesTransformer::new(rules).transform(&payload);
    let result = result.as_array().expect("array output");

    let remarks: Vec<String> = result
        .iter()
        .filter_map(|item| item.get("remarks").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    assert!(remarks.contains(&"NL+USA Balance".to_string()));
    assert!(remarks.contains(&"NL-XHTTP".to_string()));
    assert!(remarks.iter().any(|remark| remark.to_lowercase().contains("hysteria")));

    let global_config = result
        .iter()
        .find(|item| item.get("remarks") == Some(&Value::String("NL+USA Balance".to_string())))
        .expect("global config");
    let selector = &global_config["routing"]["balancers"][0]["selector"];
    assert_eq!(selector, &json!(["nl-ws", "us-ws"]));

    let outbounds_by_tag: std::collections::HashMap<String, &Value> = global_config["outbounds"]
        .as_array()
        .expect("outbounds")
        .iter()
        .filter_map(|outbound| {
            outbound
                .get("tag")
                .and_then(|v| v.as_str())
                .map(|tag| (tag.to_string(), outbound))
        })
        .collect();
    assert_eq!(
        outbounds_by_tag["nl-ws"]["settings"]["address"],
        "node1.example.com"
    );
    assert_eq!(
        outbounds_by_tag["us-ws"]["settings"]["address"],
        "node3.example.com"
    );
    assert_ne!(
        outbounds_by_tag["nl-ws"]["settings"]["address"],
        outbounds_by_tag["us-ws"]["settings"]["address"]
    );
    assert_eq!(
        global_config["routing"]["balancers"][0]["strategy"],
        json!({"type": "roundRobin"})
    );
}

#[test]
fn balancer_members_can_use_inbound_ids() {
    let payload = load_fixture("sample_nodes.json");
    let mut rules_data = base_rules();
    rules_data["tagging"]["rules"] = json!([]);
    rules_data["balancers"] = json!([
        {
            "tag": "ws-pool",
            "remarks": "WS Pool",
            "strategy": "roundRobin",
            "members": [{
                "inbound_ids": [
                    "vless|node1.example.com|ws||443||",
                    "vless|node3.example.com|ws||443||",
                ]
            }],
        }
    ]);

    let configs = payload.as_array().expect("array");
    let nodes = configs_to_nodes(configs);
    let nl_fp = nodes[0].fingerprint.clone();
    let us_fp = nodes[3].fingerprint.clone();
    rules_data["balancers"][0]["members"][0]["inbound_ids"] = json!([nl_fp, us_fp]);

    let rules = TransformRules::from_value(&rules_data).expect("rules");
    let result = RulesTransformer::new(rules).transform(&payload);
    let result = result.as_array().expect("array output");
    let ws_config = result
        .iter()
        .find(|item| item.get("remarks") == Some(&Value::String("WS Pool".to_string())))
        .expect("ws config");
    let selector = ws_config["routing"]["balancers"][0]["selector"]
        .as_array()
        .expect("selector")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(selector.len(), 2);
    assert!(selector.contains(&nl_fp));
    assert!(selector.contains(&us_fp));
}

#[test]
fn balancer_incomplete_members_adds_failed_suffix() {
    let payload = load_fixture("sample_nodes.json");
    let mut rules_data = base_rules();
    rules_data["tagging"]["rules"] = json!([]);
    rules_data["balancers"] = json!([
        {
            "tag": "mixed-pool",
            "remarks": "TESTBALANCE",
            "strategy": "roundRobin",
            "members": [{
                "inbound_ids": [
                    "vless|node1.example.com|ws||443||",
                    "vless|missing.example.com|ws||443||",
                ]
            }],
        }
    ]);

    let configs = payload.as_array().expect("array");
    let nodes = configs_to_nodes(configs);
    rules_data["balancers"][0]["members"][0]["inbound_ids"] =
        json!([nodes[0].fingerprint.clone(), "vless|missing.example.com|ws||443||"]);

    let rules = TransformRules::from_value(&rules_data).expect("rules");
    let result = RulesTransformer::new(rules).transform(&payload);
    let result = result.as_array().expect("array output");
    let failed_config = result
        .iter()
        .find(|item| {
            item.get("remarks")
                == Some(&Value::String("TESTBALANCE - Failed".to_string()))
        })
        .expect("failed config");
    assert_eq!(
        failed_config["routing"]["balancers"][0]["selector"]
            .as_array()
            .expect("selector")
            .len(),
        1
    );
}

#[test]
fn least_ping_strategy_in_balancer_output() {
    let payload = load_fixture("sample_nodes.json");
    let mut rules_data = base_rules();
    rules_data["balancers"] = json!([
        {
            "tag": "ping-pool",
            "remarks": "Fastest",
            "strategy": "leastPing",
            "members": [{"tags": ["nl-ws", "us-ws"]}],
        }
    ]);

    let rules = TransformRules::from_value(&rules_data).expect("rules");
    let result = RulesTransformer::new(rules).transform(&payload);
    let result = result.as_array().expect("array output");
    let ping_config = result
        .iter()
        .find(|item| item.get("remarks") == Some(&Value::String("Fastest".to_string())))
        .expect("ping config");
    assert_eq!(
        ping_config["routing"]["balancers"][0]["strategy"],
        json!({"type": "leastPing"})
    );
}

#[test]
fn balancer_members_can_use_remarks_match() {
    let payload = load_fixture("sample_nodes.json");
    let mut rules_data = base_rules();
    rules_data["balancers"] = json!([
        {
            "tag": "nl-pool",
            "remarks": "NL Pool",
            "strategy": "roundRobin",
            "members": [{"match": {"remarks_contains": ["NL-"]}}],
        }
    ]);

    let rules = TransformRules::from_value(&rules_data).expect("rules");
    let result = RulesTransformer::new(rules).transform(&payload);
    let result = result.as_array().expect("array output");
    let nl_config = result
        .iter()
        .find(|item| item.get("remarks") == Some(&Value::String("NL Pool".to_string())))
        .expect("nl config");
    let selector = nl_config["routing"]["balancers"][0]["selector"]
        .as_array()
        .expect("selector")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>();
    let expected: std::collections::HashSet<String> =
        ["nl-ws", "nl-xhttp", "nl-grpc"].into_iter().map(str::to_string).collect();
    assert_eq!(selector, expected);
}