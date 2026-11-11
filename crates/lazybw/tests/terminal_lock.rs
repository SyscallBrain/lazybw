//! Exercise the actual terminal runner while an external editor owns the PTY.

#![cfg(unix)]

mod harness;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct RunningTui {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    release: PathBuf,
}

impl Drop for RunningTui {
    fn drop(&mut self) {
        // Release the editor even on an assertion failure, then reap the TUI.
        let _ = std::fs::write(&self.release, "release");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for(description: &str, output: &Arc<Mutex<Vec<u8>>>, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let captured = output.lock().unwrap().clone();
    panic!("timed out waiting for {description}\n{}", String::from_utf8_lossy(&captured));
}

fn state(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn sigusr1_locks_before_external_editor_returns_and_discards_changes() {
    let dir = tempfile::TempDir::new().unwrap();
    let state_path = dir.path().join("vault.json");
    let release = dir.path().join("release");
    let started = dir.path().join("started");
    let editor = dir.path().join("editor.sh");
    let config_dir = dir.path().join("config/lazybw");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        &state_path,
        serde_json::to_vec(&json!({
            "status": "unlocked", "sessionKey": "fake-session", "folders": [],
            "items": [{"id": "test-item", "type": 1, "name": "PTY vault entry",
                "login": {"username": "fixture", "password": "synthetic-password"}}]
        }))
        .unwrap(),
    )
    .unwrap();
    // Use only synthetic data. The editor is bounded even if its parent dies.
    std::fs::write(&editor, r#"#!/bin/sh
printf started > "$EDITOR_STARTED"
count=0
while [ ! -f "$EDITOR_RELEASE" ]; do
    count=$((count + 1))
    [ "$count" -lt 400 ] || exit 1
    sleep 0.05
done
printf '%s' '{"id":"test-item","type":1,"name":"unwanted editor change","login":{"username":"fixture","password":"synthetic-password"}}' > "$1"
"#).unwrap();
    std::fs::write(config_dir.join("config.toml"), format!(
        "[bw]\ninherit_session = true\n[ui]\nanimations = false\nmouse = false\n[editor]\ncommand = 'sh {}'\n",
        editor.display()
    )).unwrap();

    let pty = native_pty_system()
        .openpty(PtySize { rows: 40, cols: 140, pixel_width: 0, pixel_height: 0 })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_lazybw"));
    command.env_clear();
    command.env("PATH", "/usr/bin:/bin");
    command.env("TERM", "xterm-256color");
    command.env("XDG_CONFIG_HOME", dir.path().join("config"));
    command.env("XDG_STATE_HOME", dir.path().join("state"));
    command.env("XDG_RUNTIME_DIR", dir.path());
    command.env("LAZYBW_BW", harness::fake_bw_path());
    command.env("BW_SESSION", "fake-session");
    command.env("FAKE_BW_STATE", &state_path);
    command.env("EDITOR_STARTED", &started);
    command.env("EDITOR_RELEASE", &release);
    let child = pty.slave.spawn_command(command).unwrap();
    let pid = child.process_id().unwrap();
    let mut running = RunningTui { child, release: release.clone() };
    drop(pty.slave);
    let mut writer = pty.master.take_writer().unwrap();
    let mut reader = pty.master.try_clone_reader().unwrap();
    let output = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&output);
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        while let Ok(count) = reader.read(&mut bytes) {
            if count == 0 {
                break;
            }
            captured.lock().unwrap().extend_from_slice(&bytes[..count]);
        }
    });
    wait_for("loaded vault", &output, || {
        String::from_utf8_lossy(&output.lock().unwrap()).contains("PTY vault entry")
    });
    writer.write_all(b":edit json\r").unwrap();
    writer.flush().unwrap();
    wait_for("external editor to start", &output, || started.exists());
    assert_eq!(state(&state_path)["status"], "unlocked");
    #[expect(unsafe_code, reason = "send SIGUSR1 only to our live PTY child")]
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1) };
    assert_eq!(sent, 0);
    wait_for("backend lock while editor is still open", &output, || {
        state(&state_path)["status"] == "locked"
    });
    assert!(!release.exists());

    std::fs::write(&release, "release").unwrap();
    wait_for("locked screen after editor returns", &output, || {
        String::from_utf8_lossy(&output.lock().unwrap()).contains("LOCKED")
    });
    writer.write_all(b"\x03").unwrap();
    writer.flush().unwrap();
    wait_for("TUI exit", &output, || running.child.try_wait().unwrap().is_some());
    let final_state = state(&state_path);
    assert_eq!(final_state["status"], "locked");
    assert_eq!(final_state["items"][0]["name"], "PTY vault entry");
    assert!(
        !final_state["log"].as_array().unwrap().iter().any(|entry| {
            entry["request"]["method"] == "PUT" || entry["request"]["path"] == "/unlock"
        }),
        "editor result must not save or unlock: {}",
        final_state["log"]
    );
}
