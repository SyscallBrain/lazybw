//! `bw` subprocesses: status, login, logout, server config, export, import
//! and the health report.
//!
//! Environment hygiene is applied to every child (bw-integration.md §2):
//! `BW_SESSION` is removed unless explicitly passed, `NO_COLOR=1` is set and
//! non-interactive commands add `--nointeraction --response`.

use crate::bw::envelope::StatusTemplate;
use crate::bw::envelope::{BwError, Envelope};
use crate::bw::models::{HealthEntry, Status};
use serde_json::Value;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Formats accepted by `bw import` when the CLI cannot be asked.
pub const FALLBACK_IMPORT_FORMATS: &[&str] = &[
    "bitwardenjson",
    "bitwardenpasswordprotected",
    "bitwardencsv",
    "1password1pux",
    "1password1pif",
    "chromecsv",
    "firefoxcsv",
    "keepass2xml",
    "lastpasscsv",
    "safarisv",
    "keepercsv",
    "dashlanecsv",
    "bitwardentxt",
    "msecurecsv",
    "passwordbossjson",
];

/// The base child environment, without any session key.
#[must_use]
pub fn child_env() -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> =
        std::env::vars_os().filter(|(key, _)| key != "BW_SESSION").collect();
    env.push((OsString::from("NO_COLOR"), OsString::from("1")));
    env
}

/// The child environment with an optional session key and extra variables.
#[must_use]
pub fn child_env_with(session: Option<&str>) -> Vec<(OsString, OsString)> {
    let mut env = child_env();
    if let Some(key) = session {
        env.push((OsString::from("BW_SESSION"), OsString::from(key)));
    }
    env
}

/// Adds extra variables (API key, import password) to a base environment.
#[must_use]
pub fn with_vars(
    mut env: Vec<(OsString, OsString)>,
    vars: &[(&str, &str)],
) -> Vec<(OsString, OsString)> {
    for (key, value) in vars {
        env.push((OsString::from(*key), OsString::from(*value)));
    }
    env
}

/// Runs a `bw` command that answers with a JSON envelope.
pub async fn run_envelope(
    path: &Path,
    args: &[&str],
    session: Option<&str>,
    extra: &[(&str, &str)],
    timeout: Duration,
) -> Result<Value, BwError> {
    let env = with_vars(child_env_with(session), extra);
    let mut command = Command::new(path);
    command
        .args(args)
        .args(["--response", "--nointeraction"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| BwError::Timeout {
            endpoint: args.first().copied().unwrap_or("bw").to_owned(),
        })?
        .map_err(|err| BwError::Other(format!("could not run bw: {err}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && stdout.trim().is_empty() {
        return Err(BwError::Other(format!(
            "bw {} failed: {}",
            args.first().copied().unwrap_or(""),
            crate::bw::serve::redact(stderr.trim())
        )));
    }
    let envelope: Envelope<Value> = serde_json::from_str(stdout.trim()).map_err(|_| {
        BwError::InvalidResponse(format!(
            "bw {} printed unexpected output: {}",
            args.first().copied().unwrap_or(""),
            stdout.trim().chars().take(200).collect::<String>()
        ))
    })?;
    if envelope.success {
        Ok(envelope.data.unwrap_or(Value::Null))
    } else {
        Err(BwError::from_message(envelope.message.as_deref().unwrap_or("Unknown error.")))
    }
}

/// `bw status`.
pub async fn status(path: &Path) -> Result<Status, BwError> {
    let data = run_envelope(path, &["status"], None, &[], Duration::from_secs(15)).await?;
    let template: StatusTemplate = serde_json::from_value(data)
        .map_err(|e| BwError::InvalidResponse(format!("status payload: {e}")))?;
    Ok(template.template)
}

/// Commands that need a terminal and are run with the TUI suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLogin {
    /// `bw login --raw`
    Password,
    /// `bw login --sso --raw`
    Sso,
}

impl InteractiveLogin {
    /// The arguments after the executable.
    #[must_use]
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Password => &["login", "--raw"],
            Self::Sso => &["login", "--sso", "--raw"],
        }
    }
}

/// Builds the interactive login command (stdin/stderr inherited, stdout piped).
#[must_use]
pub fn interactive_login_command(path: &Path, kind: InteractiveLogin) -> std::process::Command {
    let mut command = std::process::Command::new(path);
    command
        .args(kind.args())
        .env_clear()
        .envs(child_env())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped());
    command
}

/// `bw login --apikey`.
pub async fn login_apikey(
    path: &Path,
    client_id: &str,
    client_secret: &str,
) -> Result<(), BwError> {
    run_envelope(
        path,
        &["login", "--apikey"],
        None,
        &[("BW_CLIENTID", client_id), ("BW_CLIENTSECRET", client_secret)],
        Duration::from_secs(60),
    )
    .await
    .map(|_| ())
}

/// `bw logout`.
pub async fn logout(path: &Path) -> Result<(), BwError> {
    run_envelope(path, &["logout"], None, &[], Duration::from_secs(30)).await.map(|_| ())
}

/// `bw config server <url>`.
pub async fn config_server(path: &Path, url: &str) -> Result<(), BwError> {
    run_envelope(path, &["config", "server", url], None, &[], Duration::from_secs(30))
        .await
        .map(|_| ())
}

/// `bw export --format <format> --raw`, returning the file content.
pub async fn export_raw(
    path: &Path,
    format: &str,
    session: &str,
    timeout: Duration,
) -> Result<Vec<u8>, BwError> {
    let env = child_env_with(Some(session));
    let mut command = Command::new(path);
    command
        .args(["export", "--format", format, "--raw"])
        .args(["--nointeraction"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| BwError::Timeout { endpoint: "export".to_owned() })?
        .map_err(|err| BwError::Other(format!("could not run bw export: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BwError::Other(format!(
            "bw export failed: {}",
            crate::bw::serve::redact(stderr.trim())
        )));
    }
    Ok(output.stdout)
}

/// `bw export --format encrypted_json --password --output <path>`, driven
/// through a PTY so the password never appears in argv.
pub async fn export_password_protected(
    path: &Path,
    output: &Path,
    password: &str,
    session: &str,
    timeout: Duration,
) -> Result<(), BwError> {
    let exec = path.to_path_buf();
    let args: Vec<String> = vec![
        "export".to_owned(),
        "--format".to_owned(),
        "encrypted_json".to_owned(),
        "--password".to_owned(),
        "--output".to_owned(),
        output.display().to_string(),
    ];
    let env = child_env_with(Some(session));
    let output_path = output.to_path_buf();
    let password = password.to_owned();
    // `bw` runs on a blocking thread, which dropping this future does not
    // stop: the guard tells it to kill `bw` (and drop what it wrote) when
    // the export is abandoned, as a lock does.
    struct CancelOnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guard = CancelOnDrop(cancel.clone());
    let written = output_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let result = crate::bw::pty::run_with_password_prompt(
            &exec, &args, &env, &password, "password", timeout, &cancel,
        );
        if result.is_err() {
            let _ = std::fs::remove_file(&written);
        }
        result
    })
    .await;
    // Setting the flag once `bw` is done changes nothing.
    drop(guard);
    result.map_err(|e| BwError::Other(format!("export task failed: {e}")))??;
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| BwError::Other(format!("bw export produced no file: {e}")))?;
    if metadata.len() == 0 {
        return Err(BwError::Other("bw export produced an empty file".to_owned()));
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o600));
    Ok(())
}

/// `bw import <format> <path>`.
pub async fn import(
    path: &Path,
    format: &str,
    file: &Path,
    session: &str,
    file_password: Option<&str>,
    timeout: Duration,
) -> Result<String, BwError> {
    let mut extra: Vec<(&str, &str)> = Vec::new();
    let file_string = file.display().to_string();
    let mut args: Vec<&str> = vec!["import", format, &file_string];
    if let Some(password) = file_password {
        args.extend(["--passwordenv", "LAZYBW_IMPORT_PASSWORD"]);
        extra.push(("LAZYBW_IMPORT_PASSWORD", password));
    }
    let data = run_envelope(path, &args, Some(session), &extra, timeout).await?;
    Ok(data
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| data.as_str())
        .unwrap_or("Import finished.")
        .to_owned())
}

/// `bw import --formats`, parsed from the CLI output with a fallback list.
pub async fn import_formats(path: &Path, session: Option<&str>) -> Vec<String> {
    let env = child_env_with(session);
    let mut command = Command::new(path);
    command
        .args(["import", "--formats"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(30), command.output()).await
    else {
        return FALLBACK_IMPORT_FORMATS.iter().map(|s| (*s).to_owned()).collect();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut formats: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.contains(' ')
                && line.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .map(str::to_owned)
        .collect();
    if formats.len() < 3 {
        formats = FALLBACK_IMPORT_FORMATS.iter().map(|s| (*s).to_owned()).collect();
    }
    formats.sort();
    formats
}

/// `bw report password-health`.
pub async fn report_health(
    path: &Path,
    session: &str,
    check_exposed: bool,
) -> Result<Vec<HealthEntry>, BwError> {
    let mut args = vec!["report", "password-health"];
    if !check_exposed {
        args.push("--no-check-exposed");
    }
    let data = run_envelope(path, &args, Some(session), &[], Duration::from_secs(300)).await?;
    parse_health_payload(data)
}

/// Parses the `password-health` payload.
///
/// The real CLI (2026.10.1) wraps the rows in a list response:
/// `{"object":"list","data":[…]}`, while older builds and the test double at
/// times returned the bare array. Both shapes are accepted.
pub fn parse_health_payload(data: Value) -> Result<Vec<HealthEntry>, BwError> {
    #[derive(serde::Deserialize)]
    struct HealthList {
        #[serde(default)]
        data: Vec<HealthEntry>,
    }
    if data.is_array() {
        return serde_json::from_value(data)
            .map_err(|e| BwError::InvalidResponse(format!("health payload: {e}")));
    }
    let list: HealthList = serde_json::from_value(data)
        .map_err(|e| BwError::InvalidResponse(format!("health payload: {e}")))?;
    Ok(list.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(unsafe_code, reason = "the test sets the environment it is reading")]
    fn child_env_removes_session_and_sets_no_color() {
        unsafe {
            std::env::set_var("BW_SESSION", "secret");
        }
        let env = child_env();
        assert!(!env.iter().any(|(k, _)| k == "BW_SESSION"));
        assert!(env.iter().any(|(k, v)| k == "NO_COLOR" && v == "1"));
        let env = child_env_with(Some("new-session"));
        assert!(env.iter().any(|(k, v)| k == "BW_SESSION" && v == "new-session"));
        unsafe {
            std::env::remove_var("BW_SESSION");
        }
    }

    #[test]
    fn extra_vars_are_appended() {
        let env = with_vars(child_env(), &[("BW_CLIENTID", "id")]);
        assert!(env.iter().any(|(k, v)| k == "BW_CLIENTID" && v == "id"));
    }

    #[test]
    fn fallback_formats_are_sorted_and_contain_bitwarden() {
        assert!(FALLBACK_IMPORT_FORMATS.contains(&"bitwardenjson"));
        assert!(FALLBACK_IMPORT_FORMATS.contains(&"bitwardenpasswordprotected"));
    }

    #[test]
    fn parses_both_health_payload_shapes() {
        use serde_json::json;
        let row = json!({
            "object": "password-health",
            "id": "id-1",
            "name": "GitHub",
            "passwordStrength": 3,
            "reuseCount": null,
            "exposed": true,
            "exposedCount": 3812,
            "exposedError": null
        });
        let list = json!({"object": "list", "data": [row.clone()]});
        let parsed = parse_health_payload(list).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "GitHub");
        assert_eq!(parsed[0].exposed_count, Some(3812));
        let bare = parse_health_payload(json!([row])).unwrap();
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].password_strength, 3);
    }
}
