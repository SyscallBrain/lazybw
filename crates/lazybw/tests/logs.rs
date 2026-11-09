//! The log file must never contain a decrypted secret, an item password, the
//! master password or the session key.

mod harness;

use harness::TestApp;
use std::os::unix::fs::PermissionsExt;
use tracing::level_filters::LevelFilter;

/// A whole session logged at `trace`, the most a user can ask for with
/// `LAZYBW_LOG`, through the same set-up as the binary.
#[tokio::test]
async fn logs_never_contain_secrets() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("lazybw/lazybw.log");
    let guard = lazybw::logging::init_with(&path, LevelFilter::TRACE).expect("log file");
    tracing::info!("log-grep-test marker");

    let mut app = TestApp::boot(true).await;
    app.wait_screen(lazybw::app::Screen::Unlock).await;
    app.keys("correct horse<CR>").await;
    app.wait_ready().await;
    app.keys("yp").await;
    app.keys("e").await;
    app.keys(":q<CR>").await;
    app.keys("R").await;
    app.wait_idle().await;
    drop(guard);

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the log must be private");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("log-grep-test marker"), "log file was not written");
    for secret in ["correct horse", "fixture-password", "fake-session", "GEZDGNBVGY3TQOJQ"] {
        assert!(!contents.contains(secret), "secret {secret:?} leaked into the log:\n{contents}");
    }
    assert!(!contents.contains("fido2Credentials"), "request bodies leaked into the log");
}

/// An error screen is the first thing a bug report needs: what it said goes
/// into the log at `warn`, without the blank lines that space it on screen.
#[test]
fn error_screens_are_logged() {
    use lazybw::app::state::{AppState, ErrorScreen, SessionPhase};
    use lazybw::config::{Config, Paths, keymap::Keymap};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = Buffer::default();
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(LevelFilter::INFO)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let config = Config::default();
        let theme = lazybw::theme::builtin::by_name("tokyonight-night").unwrap();
        let (keymap, _) = Keymap::build(&config.keys).unwrap();
        let paths =
            Paths { config_dir: "/tmp/logs/config".into(), state_dir: "/tmp/logs/state".into() };
        let mut state = AppState::new(config, theme, keymap, paths);
        state.enter_session(SessionPhase::Failed(Box::new(ErrorScreen {
            title: "Bitwarden CLI stopped".to_owned(),
            lines: vec![
                "Lost the connection.".to_owned(),
                String::new(),
                "  Retry with r.".to_owned(),
            ],
            detail: Vec::new(),
            actions: "r retry   q quit".to_owned(),
            retry: true,
            open_file: None,
        })));
    });

    let text = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains(" WARN "), "logged at warn:\n{text}");
    assert!(text.contains("error screen: Lost the connection. / Retry with r."), "{text}");
    assert!(text.contains("title=Bitwarden CLI stopped"), "{text}");
}
