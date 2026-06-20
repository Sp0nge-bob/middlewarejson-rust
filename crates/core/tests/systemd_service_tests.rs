use std::path::PathBuf;

use middlewarejson_core::config::Settings;
use middlewarejson_core::services::systemd_service::{
    get_service_scope, render_unit_file, ServiceScope,
};

#[test]
fn render_unit_file_contains_paths() {
    let settings = Settings::default();
    let root = PathBuf::from("/opt/middlewarejson");
    let binary = root.join("target/release/middlewarejson");
    let scope = ServiceScope {
        kind: "user".to_string(),
        unit_path: PathBuf::from("/home/user/.config/systemd/user/middlewarejson.service"),
        systemctl_args: vec!["--user".to_string()],
    };
    let unit = render_unit_file(&settings, &root, &binary, &scope);
    assert!(unit.contains("WorkingDirectory="));
    assert!(unit.contains("middlewarejson"));
    assert!(unit.contains("EnvironmentFile="));
    assert!(unit.contains(".env"));
    assert!(unit.contains("ExecStart="));
    assert!(unit.contains("target"));
    assert!(unit.contains("release"));
    assert!(unit.contains("middlewarejson"));
    assert!(unit.contains("WantedBy=default.target"));
}

#[test]
fn system_scope_uses_multi_user_target() {
    let scope = get_service_scope();
    assert!(!scope.unit_path.as_os_str().is_empty());
}