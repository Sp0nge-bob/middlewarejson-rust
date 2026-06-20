use std::fs;
use std::path::PathBuf;

use middlewarejson_core::config::Settings;
use middlewarejson_core::db::{CatalogRepository, ClientRecord, Database};
use middlewarejson_core::services::transform_service::TransformService;
use serde_json::Value;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn setup_repo(db_path: &PathBuf) -> CatalogRepository {
    let repo = CatalogRepository::new(Database::new(db_path)).expect("repo");
    let nl_fp = "vless|node1.example.com|ws|/ws-path|443|tls|".to_string();
    let us_fp = "vless|node5.example.com|ws|/ws-path|443|tls|".to_string();

    repo.create_balancer(
        "premium-pool",
        "NL+USA Balance",
        "roundRobin",
        &[nl_fp, us_fp],
        "group",
        "premium",
    )
    .expect("create balancer");

    repo.upsert_clients(&[
        ClientRecord {
            sub_id: "client_a_sub_id12".to_string(),
            group_name: "premium".to_string(),
            email: "premium@example.com".to_string(),
            enable: true,
        },
        ClientRecord {
            sub_id: "client_b_sub_id12".to_string(),
            group_name: "basic".to_string(),
            email: "basic@example.com".to_string(),
            enable: true,
        },
        ClientRecord {
            sub_id: "any_sub_id_12345".to_string(),
            group_name: String::new(),
            email: "nogroup@example.com".to_string(),
            enable: true,
        },
    ])
    .expect("upsert clients");

    repo
}

fn load_raw_fixture() -> Value {
    let content = fs::read_to_string(fixture_path("raw_3xui_subscription.json")).expect("read fixture");
    serde_json::from_str(&content).expect("parse fixture")
}

fn remarks_list(result: &Value) -> Vec<String> {
    result
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|item| item.get("remarks").and_then(|v| v.as_str()).map(str::to_string))
        .collect()
}

#[test]
fn disabled_balancer_does_not_apply() {
    let db_path = std::env::temp_dir().join("middlewarejson_disabled.db");
    let _ = std::fs::remove_file(&db_path);
    let repo = setup_repo(&db_path);
    repo.set_balancer_scope("premium-pool", "disabled", "")
        .expect("set scope");

    let mut settings = Settings::default();
    settings.transform_mode = "rules".to_string();
    settings.db_path = db_path.clone();

    let service = TransformService::new(settings).expect("service");
    let result = service
        .transform("client_a_sub_id12", &load_raw_fixture())
        .expect("transform");
    assert!(!remarks_list(&result).contains(&"NL+USA Balance".to_string()));
}

#[test]
fn all_scope_applies_to_everyone() {
    let db_path = std::env::temp_dir().join("middlewarejson_all.db");
    let _ = std::fs::remove_file(&db_path);
    let repo = setup_repo(&db_path);
    repo.set_balancer_scope("premium-pool", "all", "")
        .expect("set scope");

    let mut settings = Settings::default();
    settings.transform_mode = "rules".to_string();
    settings.db_path = db_path;

    let service = TransformService::new(settings).expect("service");
    let configs = load_raw_fixture();
    for sub_id in [
        "client_a_sub_id12",
        "client_b_sub_id12",
        "any_sub_id_12345",
    ] {
        let result = service.transform(sub_id, &configs).expect("transform");
        assert!(remarks_list(&result).contains(&"NL+USA Balance".to_string()));
    }
}

#[test]
fn client_and_group_balancers_both_apply() {
    let db_path = std::env::temp_dir().join("middlewarejson_client.db");
    let _ = std::fs::remove_file(&db_path);
    let repo = CatalogRepository::new(Database::new(&db_path)).expect("repo");
    let nl_fp = "vless|node1.example.com|ws|/ws-path|443|tls|".to_string();
    let us_fp = "vless|node5.example.com|ws|/ws-path|443|tls|".to_string();

    repo.create_balancer(
        "group-pool",
        "Group Pool",
        "roundRobin",
        std::slice::from_ref(&nl_fp),
        "group",
        "premium",
    )
    .expect("create group balancer");
    repo.create_balancer(
        "solo-pool",
        "Solo Pool",
        "leastLoad",
        std::slice::from_ref(&us_fp),
        "client",
        "client_a_sub_id12",
    )
    .expect("create solo balancer");
    repo.upsert_clients(&[
        ClientRecord {
            sub_id: "client_a_sub_id12".to_string(),
            group_name: "premium".to_string(),
            email: "premium@example.com".to_string(),
            enable: true,
        },
        ClientRecord {
            sub_id: "client_b_sub_id12".to_string(),
            group_name: "premium".to_string(),
            email: "basic@example.com".to_string(),
            enable: true,
        },
    ])
    .expect("upsert clients");

    let mut settings = Settings::default();
    settings.transform_mode = "rules".to_string();
    settings.db_path = db_path;

    let service = TransformService::new(settings).expect("service");
    let configs = load_raw_fixture();

    let solo_result = service
        .transform("client_a_sub_id12", &configs)
        .expect("transform");
    let solo_remarks = remarks_list(&solo_result);
    assert!(solo_remarks.contains(&"Solo Pool".to_string()));
    assert!(solo_remarks.contains(&"Group Pool".to_string()));

    let group_result = service
        .transform("client_b_sub_id12", &configs)
        .expect("transform");
    let group_remarks = remarks_list(&group_result);
    assert!(group_remarks.contains(&"Group Pool".to_string()));
    assert!(!group_remarks.contains(&"Solo Pool".to_string()));
}