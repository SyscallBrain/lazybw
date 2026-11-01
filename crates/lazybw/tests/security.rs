//! Security tests: secrets never reach argv, logs or messages.

mod harness;

use harness::TestApp;

const SECRET: &str = "correct horse";
const ITEM_SECRET: &str = "fixture-password";

#[tokio::test]
async fn unlock_password_never_appears_in_argv() {
    let mut app = TestApp::boot(true).await;
    app.wait_screen(lazybw::app::Screen::Unlock).await;
    app.keys("correct horse<CR>").await;
    app.wait_ready().await;
    for entry in app.requests() {
        let argv = serde_json::to_string(&entry["cmd"]).unwrap();
        assert!(!argv.contains(SECRET), "password in argv: {argv}");
        assert!(!argv.contains("fake-session"), "session key in argv: {argv}");
    }
}

#[tokio::test]
async fn item_secrets_never_appear_in_messages() {
    let mut app = TestApp::boot(false).await;
    app.wait_ready().await;
    app.keys("yp").await;
    app.keys("gh").await;
    app.keys("<Esc>").await;
    app.keys("K").await;
    app.keys("<Esc>").await;
    let messages =
        app.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n");
    assert!(!messages.contains(ITEM_SECRET), "secret in messages: {messages}");
    assert!(!messages.contains("GEZDGNBVGY3TQOJQ"), "TOTP key in messages");
}

#[tokio::test]
async fn fake_bw_logs_environment_names_only() {
    let app = TestApp::boot(false).await;
    for entry in app.requests() {
        let env = entry["env"].as_array().cloned().unwrap_or_default();
        for name in env {
            let name = name.as_str().unwrap_or_default();
            assert!(
                name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()),
                "environment value leaked into the log: {name}"
            );
        }
    }
}

#[tokio::test]
async fn clipboard_auto_clears() {
    let mut app = TestApp::boot(false).await;
    app.wait_ready().await;
    app.keys("yp").await;
    assert!(app.app.state.clipboard.has_pending());
    // Fast-forward: the memory backend must be cleared by the tick.
    let later = std::time::Instant::now() + std::time::Duration::from_secs(31);
    assert!(app.app.state.clipboard.tick(later));
    assert_eq!(app.app.state.clipboard.peek(), None);
}

/// Helper process for the PDEATHSIG test. Run normally, it does nothing.
#[test]
fn pdeathsig_helper() {
    if std::env::var("LAZYBW_PDEATHSIG_HELPER").is_err() {
        return;
    }
    use lazybw::bw::serve::{Serve, ServeOptions};
    use lazybw::config::Transport;
    use std::io::Write;
    let dir = tempfile::TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let state = serde_json::json!({
        "version": "2026.10.1", "status": "unlocked",
        "email": "you@example.com", "masterPassword": "correct horse",
        "sessionKey": "fake-session", "folders": [], "items": [], "log": []
    });
    std::fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    #[expect(unsafe_code, reason = "the test sets the environment the child inherits")]
    unsafe {
        std::env::set_var("FAKE_BW_STATE", &state_path);
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let serve = Serve::spawn(&ServeOptions {
            path: harness::fake_bw_path(),
            session_key: None,
            transport: Transport::Fd,
            runtime_dir: dir.path().to_path_buf(),
        })
        .expect("spawn serve");
        println!("SERVE_PID {}", serve.pid().expect("pid"));
        std::io::stdout().flush().unwrap();
        // Keep the child (and ourselves) alive until the parent SIGKILLs us.
        std::mem::forget(serve);
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    });
}

#[test]
fn sigkill_of_the_parent_terminates_the_serve_child() {
    use std::io::BufRead;
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    std::fs::write(
        &state_path,
        br#"{"version":"2026.10.1","status":"unlocked","email":"you@example.com","masterPassword":"correct horse","sessionKey":"fake-session","folders":[],"items":[],"log":[]}"#,
    )
    .unwrap();
    let mut child = std::process::Command::new(exe)
        .args(["--exact", "pdeathsig_helper", "--nocapture"])
        .env("LAZYBW_PDEATHSIG_HELPER", "1")
        .env("FAKE_BW_STATE", &state_path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn helper");
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut serve_pid = None;
    let mut line = String::new();
    for _ in 0..100 {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(pid) = line.trim().strip_prefix("SERVE_PID ") {
            serve_pid = pid.parse::<u32>().ok();
            break;
        }
    }
    let serve_pid = serve_pid.expect("helper did not report the serve pid");
    assert!(std::path::Path::new(&format!("/proc/{serve_pid}")).exists());
    // SIGKILL the parent; PR_SET_PDEATHSIG in the child must terminate it.
    #[expect(unsafe_code, reason = "SIGKILL the parent to prove PR_SET_PDEATHSIG works")]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
    let mut gone = false;
    for _ in 0..100 {
        if !std::path::Path::new(&format!("/proc/{serve_pid}")).exists() {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(gone, "fake-bw serve ({serve_pid}) survived a SIGKILL of its parent");
}
