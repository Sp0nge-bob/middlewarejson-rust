//! Управление systemd-службой middlewarejson.
//!
//! Production API: meaningful only on Linux (systemd + journalctl).
//! Non-Linux stubs exist so `cargo test` for core can run elsewhere.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Settings;

pub const SERVICE_NAME: &str = "middlewarejson";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceScope {
    pub kind: String,
    pub unit_path: PathBuf,
    pub systemctl_args: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceStatus {
    pub installed: bool,
    pub active: String,
    pub enabled: Option<String>,
    pub main_pid: Option<String>,
    pub unit_path: Option<PathBuf>,
    pub scope: Option<ServiceScope>,
    pub journal_tail: Vec<String>,
    pub error: Option<String>,
}

pub fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

pub fn systemctl_available() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_root_user() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

pub fn detect_project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn detect_server_binary(root: &Path) -> PathBuf {
    let release = root.join("target/release/middlewarejson");
    if release.is_file() {
        return release;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("middlewarejson");
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    release
}

pub fn get_service_scope() -> ServiceScope {
    #[cfg(target_os = "linux")]
    {
        if is_root_user() {
            return ServiceScope {
                kind: "system".to_string(),
                unit_path: PathBuf::from(format!("/etc/systemd/system/{SERVICE_NAME}.service")),
                systemctl_args: vec![],
            };
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        ServiceScope {
            kind: "user".to_string(),
            unit_path: PathBuf::from(home)
                .join(".config/systemd/user")
                .join(format!("{SERVICE_NAME}.service")),
            systemctl_args: vec!["--user".to_string()],
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        ServiceScope {
            kind: "unknown".to_string(),
            unit_path: PathBuf::from(format!("/etc/systemd/system/{SERVICE_NAME}.service")),
            systemctl_args: vec![],
        }
    }
}

pub fn render_unit_file(
    _settings: &Settings,
    project_root: &Path,
    binary_path: &Path,
    scope: &ServiceScope,
) -> String {
    let env_file = project_root.join(".env");
    let wanted_by = if scope.kind == "user" {
        "default.target"
    } else {
        "multi-user.target"
    };
    let user_line = if scope.kind == "user" {
        String::new()
    } else {
        let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        format!("User={user}\n")
    };

    format!(
        "[Unit]\n\
         Description=middlewarejson — 3x-ui JSON subscription proxy\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         {user_line}\
         WorkingDirectory={}\n\
         EnvironmentFile={}\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy={wanted_by}\n",
        project_root.display(),
        env_file.display(),
        binary_path.display(),
    )
}

fn run_systemctl(scope: &ServiceScope, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("systemctl");
    for arg in &scope.systemctl_args {
        command.arg(arg);
    }
    for arg in args {
        command.arg(arg);
    }
    command.output().unwrap_or_else(|e| {
        std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: format!("{e}").into_bytes(),
        }
    })
}

pub fn read_service_status(scope: Option<&ServiceScope>) -> ServiceStatus {
    if !is_linux() {
        return ServiceStatus {
            error: Some("Доступно только на Linux VPS".to_string()),
            active: "unknown".to_string(),
            ..Default::default()
        };
    }
    if !systemctl_available() {
        return ServiceStatus {
            error: Some("systemctl не найден".to_string()),
            active: "unknown".to_string(),
            ..Default::default()
        };
    }

    let scope = scope.cloned().unwrap_or_else(get_service_scope);
    if !scope.unit_path.exists() {
        return ServiceStatus {
            installed: false,
            active: "not-found".to_string(),
            unit_path: Some(scope.unit_path.clone()),
            scope: Some(scope),
            ..Default::default()
        };
    }

    let show = run_systemctl(
        &scope,
        &[
            "show",
            SERVICE_NAME,
            "--property",
            "ActiveState,UnitFileState,MainPID",
            "--no-pager",
        ],
    );
    if !show.status.success() {
        let message = String::from_utf8_lossy(&show.stderr);
        let stdout = String::from_utf8_lossy(&show.stdout);
        return ServiceStatus {
            installed: true,
            active: "unknown".to_string(),
            unit_path: Some(scope.unit_path.clone()),
            scope: Some(scope.clone()),
            error: Some(if message.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                message.trim().to_string()
            }),
            ..Default::default()
        };
    }

    let mut props = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&show.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            props.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    let mut journal_tail = Vec::new();
    if Command::new("journalctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let mut cmd = Command::new("journalctl");
        for arg in &scope.systemctl_args {
            cmd.arg(arg);
        }
        let journal = cmd
            .args(["-u", SERVICE_NAME, "-n", "5", "--no-pager"])
            .output();
        if let Ok(output) = journal {
            if output.status.success() {
                journal_tail = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::to_string)
                    .collect();
            }
        }
    }

    let main_pid = props.get("MainPID").cloned();
    ServiceStatus {
        installed: true,
        active: props
            .get("ActiveState")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        enabled: props.get("UnitFileState").cloned(),
        main_pid: main_pid.filter(|pid| !pid.is_empty() && pid != "0"),
        unit_path: Some(scope.unit_path.clone()),
        scope: Some(scope),
        journal_tail,
        error: None,
    }
}

pub fn start_service(scope: Option<&ServiceScope>) -> (bool, String) {
    let scope = scope.cloned().unwrap_or_else(get_service_scope);
    let result = run_systemctl(&scope, &["start", SERVICE_NAME]);
    if result.status.success() {
        (true, "Служба запущена".to_string())
    } else {
        let message = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        (
            false,
            if message.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                message.trim().to_string()
            },
        )
    }
}

pub fn restart_service(scope: Option<&ServiceScope>) -> (bool, String) {
    let scope = scope.cloned().unwrap_or_else(get_service_scope);
    let result = run_systemctl(&scope, &["restart", SERVICE_NAME]);
    if result.status.success() {
        (true, "Служба перезапущена".to_string())
    } else {
        let message = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        (
            false,
            if message.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                message.trim().to_string()
            },
        )
    }
}

pub fn install_service(
    settings: &Settings,
    project_root: &Path,
    start_after: bool,
) -> (bool, String) {
    if !is_linux() {
        return (false, "Доступно только на Linux VPS".to_string());
    }
    if !systemctl_available() {
        return (false, "systemctl не найден".to_string());
    }

    let binary = detect_server_binary(project_root);
    let env_file = project_root.join(".env");
    if !binary.is_file() {
        return (
            false,
            format!(
                "Не найден {} — выполните cargo build --release",
                binary.display()
            ),
        );
    }
    if !env_file.is_file() {
        return (false, format!("Не найден {}", env_file.display()));
    }

    let scope = get_service_scope();
    if let Some(parent) = scope.unit_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let unit_content = render_unit_file(settings, project_root, &binary, &scope);
    if std::fs::write(&scope.unit_path, unit_content).is_err() {
        return (false, "Не удалось записать unit-файл".to_string());
    }

    let reload = run_systemctl(&scope, &["daemon-reload"]);
    if !reload.status.success() {
        let message = String::from_utf8_lossy(&reload.stderr);
        return (false, message.trim().to_string());
    }

    let enable = run_systemctl(&scope, &["enable", SERVICE_NAME]);
    if !enable.status.success() {
        let message = String::from_utf8_lossy(&enable.stderr);
        return (false, message.trim().to_string());
    }

    if start_after {
        let (started, message) = start_service(Some(&scope));
        if !started {
            return (false, message);
        }
    }

    if scope.kind == "user" {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        return (
            true,
            format!(
                "Служба установлена (user). Для работы без входа: loginctl enable-linger {user}"
            ),
        );
    }

    (true, "Служба установлена (system)".to_string())
}