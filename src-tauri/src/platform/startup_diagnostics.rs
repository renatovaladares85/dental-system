use std::{
    fs::{self, File, OpenOptions},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::Path,
};

use serde::Serialize;

use crate::{
    infrastructure::{ProtectedKeyFile, runtime_security_diagnostics},
    platform::host_identity::HostIdentityManager,
};

const DATABASE_PATH: [&str; 3] = ["Data", "active", "database.sqlcipher"];
const PROTECTED_KEY_PATH: [&str; 3] = ["Data", "active", "database-key.dpapi.json"];
const RUNTIME_LOG_PATH: [&str; 2] = ["logs", "runtime"];
const LOCK_FILE_NAME: &str = ".offline-dental-system.lock";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnostics {
    pub format_version: u16,
    pub product: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
    pub program_data_available: bool,
    pub product_root_exists: bool,
    pub data_directory_exists: bool,
    pub data_directory_writable: bool,
    pub instance_lock_available: bool,
    pub admin_port_available: bool,
    pub lan_port_available: bool,
    pub host_identity_state: &'static str,
    pub database_state: &'static str,
    pub protected_key_state: &'static str,
    pub tls_state: &'static str,
    pub runtime_log_directory_available: bool,
    pub sqlcipher_version: String,
    pub distribution_ready: bool,
    pub overall_status: &'static str,
    pub issues: Vec<StartupDiagnosticIssue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnosticIssue {
    pub code: &'static str,
    pub severity: &'static str,
}

pub fn collect(product_root: Option<&Path>, program_data_available: bool) -> StartupDiagnostics {
    let product_root_exists = product_root.is_some_and(Path::is_dir);
    let data_directory = product_root.map(|root| root.join("Data"));
    let data_directory_exists = data_directory.as_deref().is_some_and(Path::is_dir);
    let data_directory_writable = data_directory
        .as_deref()
        .is_some_and(directory_is_not_read_only);
    let host_identity_state = product_root.map_or("missing", host_identity_state);
    let database_state = product_root.map_or("missing", database_state);
    let protected_key_state = product_root.map_or("missing", protected_key_state);
    let tls_state = product_root.map_or("missing", tls_state);
    let runtime_log_directory_available = product_root
        .map(|root| root.join(RUNTIME_LOG_PATH[0]).join(RUNTIME_LOG_PATH[1]))
        .as_deref()
        .is_some_and(directory_is_not_read_only);
    let instance_lock_available = product_root.is_some_and(instance_lock_available);
    let admin_port_available = port_is_available(8742);
    let lan_port_available = port_is_available(8743);
    let runtime_diagnostics = runtime_security_diagnostics().ok();
    let sqlcipher_version = runtime_diagnostics
        .as_ref()
        .map(|diagnostics| diagnostics.sqlcipher_version.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    let distribution_ready = runtime_diagnostics
        .as_ref()
        .is_some_and(|diagnostics| diagnostics.distribution_ready);

    let mut issues = Vec::new();
    if !program_data_available {
        issues.push(issue("STARTUP_PROGRAM_DATA_UNAVAILABLE", "error"));
    }
    if !product_root_exists {
        issues.push(issue("STARTUP_PRODUCT_ROOT_MISSING", "warning"));
    }
    if !data_directory_exists {
        issues.push(issue("STARTUP_DATA_DIRECTORY_MISSING", "warning"));
    } else if !data_directory_writable {
        issues.push(issue("STARTUP_DATA_DIRECTORY_UNAVAILABLE", "error"));
    }
    if !instance_lock_available {
        issues.push(issue("STARTUP_INSTANCE_LOCK_UNAVAILABLE", "error"));
    }
    if !admin_port_available {
        issues.push(issue("STARTUP_ADMIN_PORT_IN_USE", "error"));
    }
    if !lan_port_available {
        issues.push(issue("STARTUP_LAN_PORT_IN_USE", "error"));
    }
    add_state_issue(
        &mut issues,
        "STARTUP_HOST_IDENTITY",
        host_identity_state,
        true,
    );
    add_state_issue(&mut issues, "STARTUP_DATABASE", database_state, false);
    add_state_issue(
        &mut issues,
        "STARTUP_PROTECTED_KEY",
        protected_key_state,
        false,
    );
    add_state_issue(&mut issues, "STARTUP_TLS", tls_state, false);
    if !runtime_log_directory_available {
        issues.push(issue(
            "STARTUP_RUNTIME_LOG_DIRECTORY_UNAVAILABLE",
            "warning",
        ));
    }
    if runtime_diagnostics.is_none() {
        issues.push(issue(
            "STARTUP_SQLCIPHER_DIAGNOSTICS_UNAVAILABLE",
            "warning",
        ));
    }

    let overall_status = if issues.iter().any(|item| item.severity == "error") {
        "blocked"
    } else if issues.is_empty() {
        "ok"
    } else {
        "warning"
    };

    StartupDiagnostics {
        format_version: 1,
        product: "Offline Dental System",
        version: env!("CARGO_PKG_VERSION"),
        platform: "windows-x64",
        program_data_available,
        product_root_exists,
        data_directory_exists,
        data_directory_writable,
        instance_lock_available,
        admin_port_available,
        lan_port_available,
        host_identity_state,
        database_state,
        protected_key_state,
        tls_state,
        runtime_log_directory_available,
        sqlcipher_version,
        distribution_ready,
        overall_status,
        issues,
    }
}

fn issue(code: &'static str, severity: &'static str) -> StartupDiagnosticIssue {
    StartupDiagnosticIssue { code, severity }
}

fn add_state_issue(
    issues: &mut Vec<StartupDiagnosticIssue>,
    prefix: &'static str,
    state: &'static str,
    missing_is_error: bool,
) {
    match state {
        "invalid" | "unreadable" => issues.push(issue(
            match prefix {
                "STARTUP_HOST_IDENTITY" => "STARTUP_HOST_IDENTITY_INVALID",
                "STARTUP_DATABASE" => "STARTUP_DATABASE_UNREADABLE",
                "STARTUP_PROTECTED_KEY" => "STARTUP_PROTECTED_KEY_INVALID",
                "STARTUP_TLS" => "STARTUP_TLS_INVALID",
                _ => unreachable!("startup diagnostic prefix is fixed"),
            },
            "error",
        )),
        "missing" if missing_is_error => {
            issues.push(issue("STARTUP_HOST_IDENTITY_MISSING", "error"))
        }
        "missing" => issues.push(issue(
            match prefix {
                "STARTUP_DATABASE" => "STARTUP_DATABASE_MISSING",
                "STARTUP_PROTECTED_KEY" => "STARTUP_PROTECTED_KEY_MISSING",
                "STARTUP_TLS" => "STARTUP_TLS_MISSING",
                _ => unreachable!("startup diagnostic prefix is fixed"),
            },
            "warning",
        )),
        "unknown" | "present" | "valid" => {}
        _ => unreachable!("startup diagnostic state is fixed"),
    }
}

fn directory_is_not_read_only(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
        .unwrap_or(false)
}

fn instance_lock_available(product_root: &Path) -> bool {
    let lock_path = product_root.join(LOCK_FILE_NAME);
    match OpenOptions::new().read(true).write(true).open(lock_path) {
        Ok(file) => {
            let available = file.try_lock().is_ok();
            let _ = file.unlock();
            available
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            directory_is_not_read_only(product_root)
        }
        Err(_) => false,
    }
}

fn port_is_available(port: u16) -> bool {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
}

fn host_identity_state(product_root: &Path) -> &'static str {
    let path = product_root.join("host-identity.json");
    if !path.exists() {
        return "missing";
    }
    if HostIdentityManager::read(&path).is_ok() {
        "valid"
    } else {
        "invalid"
    }
}

fn database_state(product_root: &Path) -> &'static str {
    let path = product_root
        .join(DATABASE_PATH[0])
        .join(DATABASE_PATH[1])
        .join(DATABASE_PATH[2]);
    if !path.exists() {
        return "missing";
    }
    if File::open(path).is_ok() {
        "present"
    } else {
        "unreadable"
    }
}

fn protected_key_state(product_root: &Path) -> &'static str {
    let path = product_root
        .join(PROTECTED_KEY_PATH[0])
        .join(PROTECTED_KEY_PATH[1])
        .join(PROTECTED_KEY_PATH[2]);
    if !path.exists() {
        return "missing";
    }
    if ProtectedKeyFile::read(&path).is_ok() {
        "present"
    } else {
        "invalid"
    }
}

fn tls_state(product_root: &Path) -> &'static str {
    let tls_directory = product_root.join("tls");
    if !tls_directory.exists() {
        return "missing";
    }
    let entries = match fs::read_dir(&tls_directory) {
        Ok(entries) => entries,
        Err(_) => return "invalid",
    };
    let mut has_envelope = false;
    let mut has_ca = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        has_envelope |= name.starts_with("identity-v1-") && name.ends_with(".json");
        has_ca |= name == "ca.cer";
    }
    if has_envelope && has_ca {
        "present"
    } else {
        "invalid"
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, net::TcpListener};

    use super::collect;

    #[test]
    fn diagnostics_do_not_create_a_missing_product_root() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let product_root = temporary.path().join("OfflineDentalSystem");

        let diagnostics = collect(Some(&product_root), true);

        assert!(!product_root.exists());
        assert!(!diagnostics.product_root_exists);
        assert_eq!(diagnostics.host_identity_state, "missing");
    }

    #[test]
    fn diagnostics_report_invalid_identity_and_present_database_without_opening_it() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let product_root = temporary.path();
        let active = product_root.join("Data").join("active");
        fs::create_dir_all(&active).expect("active data directory");
        fs::write(product_root.join("host-identity.json"), b"invalid").expect("identity");
        fs::write(active.join("database.sqlcipher"), b"opaque database").expect("database");

        let diagnostics = collect(Some(product_root), true);

        assert_eq!(diagnostics.host_identity_state, "invalid");
        assert_eq!(diagnostics.database_state, "present");
        assert_eq!(diagnostics.protected_key_state, "missing");
    }

    #[test]
    fn diagnostics_report_an_occupied_admin_port() {
        let listener = TcpListener::bind("127.0.0.1:8742").expect("reserve admin port");
        let temporary = tempfile::tempdir().expect("temporary directory");

        let diagnostics = collect(Some(temporary.path()), true);

        assert!(!diagnostics.admin_port_available);
        assert!(
            diagnostics
                .issues
                .iter()
                .any(|issue| issue.code == "STARTUP_ADMIN_PORT_IN_USE")
        );
        drop(listener);
    }
}
