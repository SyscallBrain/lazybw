//! Opt-in checks against the official CLI, with no account or credentials.
//! Run through scripts/check-bw.py so HOME and appdata are disposable.

use lazybw::bw::serve::{Serve, ServeOptions};
use lazybw::bw::{BwError, locate, subprocess};
use lazybw::config::Transport;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires official bw; run scripts/check-bw.py BW_PATH EXPECTED_VERSION"]
async fn official_cli_logged_out_compatibility() {
    assert!(
        !std::env::vars_os().any(|(k, _)| {
            let key = k.to_string_lossy();
            key.starts_with("BW_") || key.starts_with("LAZYBW_")
        }),
        "run through scripts/check-bw.py; inherited Bitwarden settings are forbidden"
    );
    let path = std::env::var("TEST_BW_PATH").expect("run through scripts/check-bw.py");
    let expected = std::env::var("TEST_BW_EXPECTED_VERSION").expect("expected official bw version");
    let appdata = PathBuf::from(std::env::var_os("BITWARDENCLI_APPDATA_DIR").unwrap());
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(appdata.starts_with(&home));
    assert_eq!(std::fs::read_dir(&appdata).unwrap().count(), 0, "appdata must start empty");

    let located = locate::locate(&path).expect("official CLI version must be supported");
    assert_eq!(located.capabilities.version.to_string(), expected);
    // Exercises lazybw's actual command, envelope parsing and status model.
    let status = subprocess::status(&located.path).await.expect("parse official bw status");
    assert_eq!(status.status, "unauthenticated");
    assert!(status.user_email.is_none() && status.user_id.is_none());

    for transport in [Transport::Fd, Transport::Unix] {
        let mut serve = Serve::spawn(&ServeOptions {
            path: located.path.clone(),
            session_key: None,
            transport,
            runtime_dir: PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap()),
        })
        .expect("start official bw serve");
        let result = tokio::time::timeout(Duration::from_secs(45), serve.connect()).await;
        // Both tested versions deliberately refuse serve before login. A
        // timeout, missing executable, invalid JSON or any other error fails.
        assert!(
            matches!(result, Ok(Err(BwError::NotLoggedIn))),
            "unexpected serve result: {result:?}"
        );
        assert!(serve.stderr_tail().contains("You are not logged in."));
        serve.shutdown().await;
    }
    println!(
        "official bw {expected}: version, logged-out status and both serve rejections verified"
    );
}
