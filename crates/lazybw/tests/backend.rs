//! Backend transport tests: `bw serve` over a Unix socket and the fd
//! socketpair, talking to `fake-bw`.

mod harness;

use harness::fake_bw_path;
use lazybw::bw::models::GenerateRequest;
use lazybw::bw::serve::{Serve, ServeOptions};
use lazybw::config::Transport;
use serde_json::json;

fn state_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("state.json");
    let state = json!({
        "version": "2026.10.1",
        "status": "unlocked",
        "email": "you@example.com",
        "masterPassword": "correct horse",
        "sessionKey": "fake-session",
        "folders": [{"object": "folder", "id": "f1", "name": "Work"}],
        "items": [],
        "log": []
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    path
}

/// The tests point fake-bw at their state file through a process-wide
/// environment variable, so they must not run at the same time.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn roundtrip(transport: Transport) {
    let _env = ENV_LOCK.lock().await;
    let dir = tempfile::TempDir::new().unwrap();
    let state_path = state_file(dir.path());
    // The child inherits this environment.
    #[expect(unsafe_code, reason = "the test sets the environment the child inherits")]
    unsafe {
        std::env::set_var("FAKE_BW_STATE", &state_path);
    }
    let mut serve = Serve::spawn(&ServeOptions {
        path: fake_bw_path(),
        session_key: None,
        transport,
        runtime_dir: dir.path().to_path_buf(),
    })
    .expect("spawn serve");
    let mut client = serve.connect().await.expect("connect");
    let status = client.status().await.expect("status");
    assert!(status.is_unlocked());
    let folders = client.list_folders().await.expect("folders");
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "Work");

    let item = client
        .create_item(&lazybw::bw::models::Item::new(lazybw::bw::models::ItemType::Login))
        .await
        .expect("create item");
    assert!(item.id.is_some());
    let id = item.id.clone().unwrap();
    let mut fetched = client.get_item(&id).await.expect("get item");
    assert_eq!(fetched.kind, lazybw::bw::models::ItemType::Login);
    fetched.name = "Renamed".to_owned();
    let updated = client.put_item(&id, &fetched).await.expect("put item");
    assert_eq!(updated.name, "Renamed");
    client.delete_item(&id, false).await.expect("trash");
    let trash = client.list_items(true, false).await.expect("trash list");
    assert_eq!(trash.len(), 1);
    client.restore_item(&id).await.expect("restore");
    let items = client.list_items(false, false).await.expect("items");
    assert_eq!(items.len(), 1);
    client.delete_item(&id, true).await.expect("purge");
    let items = client.list_items(false, false).await.expect("items");
    assert!(items.is_empty());

    let password = client
        .generate(&GenerateRequest { length: 32, ..GenerateRequest::default() })
        .await
        .expect("generate");
    assert_eq!(password.chars().count(), 32);

    let pid = serve.pid();
    serve.shutdown().await;
    // The child exits when the socket closes (Unix) or is terminated.
    if let Some(pid) = pid {
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "fake-bw serve ({pid}) is still running"
        );
    }
}

#[tokio::test]
async fn serve_roundtrip_unix() {
    roundtrip(Transport::Unix).await;
}

#[tokio::test]
async fn serve_roundtrip_fd() {
    roundtrip(Transport::Fd).await;
}
