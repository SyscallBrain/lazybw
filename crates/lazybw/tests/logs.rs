//! The log file must never contain a decrypted secret, an item password, the
//! master password or the session key.

mod harness;

use harness::TestApp;
use std::io::Read;

#[tokio::test]
async fn logs_never_contain_secrets() {
    let dir = tempfile::TempDir::new().unwrap();
    let appender = tracing_appender::rolling::never(dir.path(), "lazybw.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .try_init();
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

    let mut contents = String::new();
    for entry in std::fs::read_dir(dir.path()).unwrap().flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "lazybw.log") {
            std::fs::File::open(&path).unwrap().read_to_string(&mut contents).unwrap();
        }
    }
    assert!(contents.contains("log-grep-test marker"), "log file was not written");
    for secret in ["correct horse", "fixture-password", "fake-session", "GEZDGNBVGY3TQOJQ"] {
        assert!(!contents.contains(secret), "secret {secret:?} leaked into the log:\n{contents}");
    }
    assert!(!contents.contains("fido2Credentials"), "request bodies leaked into the log");
}
