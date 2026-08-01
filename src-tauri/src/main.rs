use std::{ffi::OsString, path::PathBuf, sync::Arc};

const USAGE: &str = "Uso: offline-dental-system --service | --console --data-directory <caminho-absoluto-Data> | --security-diagnostics --json";

enum Mode {
    Service,
    Console(PathBuf),
    SecurityDiagnostics,
    Help,
}

fn main() {
    if let Err(code) = run_main() {
        eprintln!("{code}\n{USAGE}");
        std::process::exit(2);
    }
}

fn run_main() -> Result<(), &'static str> {
    match parse_mode(std::env::args_os().skip(1).collect())? {
        Mode::Service => {
            offline_dental_system_lib::platform::windows_service::run_dispatcher(Arc::new(
                offline_dental_system_lib::run_as_windows_service,
            ))
            .map_err(|error| error.code())?;
        }
        Mode::Console(data_directory) => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|_| "RUNTIME_START_FAILED")?
                .block_on(offline_dental_system_lib::run_console(data_directory))
                .map_err(|error| offline_dental_system_lib::startup_error_code(error.as_ref()))?;
        }
        Mode::SecurityDiagnostics => {
            let json = offline_dental_system_lib::security_diagnostics_json()
                .map_err(|_| "SECURITY_DIAGNOSTICS_FAILED")?;
            println!("{json}");
        }
        Mode::Help => println!("{USAGE}"),
    }
    Ok(())
}

fn parse_mode(arguments: Vec<OsString>) -> Result<Mode, &'static str> {
    if arguments.len() == 1 && arguments[0] == "--service" {
        return Ok(Mode::Service);
    }
    if arguments.len() == 2 && arguments[0] == "--security-diagnostics" && arguments[1] == "--json"
    {
        return Ok(Mode::SecurityDiagnostics);
    }
    if arguments.len() == 3
        && arguments[0] == "--console"
        && arguments[1] == "--data-directory"
        && !arguments[2].is_empty()
    {
        return Ok(Mode::Console(PathBuf::from(&arguments[2])));
    }
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h")) {
        return Ok(Mode::Help);
    }
    Err("INVALID_COMMAND_LINE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_mixed_or_implicit_modes() {
        assert!(parse_mode(Vec::new()).is_err());
        assert!(parse_mode(vec!["--service".into(), "--console".into()]).is_err());
        assert!(parse_mode(vec!["--security-diagnostics".into()]).is_err());
    }

    #[test]
    fn parser_accepts_the_three_explicit_operational_modes() {
        assert!(matches!(
            parse_mode(vec!["--service".into()]).expect("service"),
            Mode::Service
        ));
        assert!(matches!(
            parse_mode(vec!["--security-diagnostics".into(), "--json".into()])
                .expect("diagnostics"),
            Mode::SecurityDiagnostics
        ));
        assert!(matches!(
            parse_mode(vec![
                "--console".into(),
                "--data-directory".into(),
                "/tmp/ods/Data".into(),
            ])
            .expect("console"),
            Mode::Console(_)
        ));
    }
}
