use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Debug)]
struct DelayedClipboard {
    memory: MemoryClipboard,
    first: AtomicBool,
    started: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    writes: Mutex<Vec<String>>,
}

impl ClipboardBackend for DelayedClipboard {
    fn set(&self, text: &str) -> Result<(), String> {
        if self.first.swap(false, Ordering::SeqCst) {
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .map_err(|e| e.to_string())?;
        }
        self.writes.lock().unwrap().push(text.to_owned());
        self.memory.set(text)
    }
    fn read(&self) -> Result<Option<String>, String> {
        self.memory.read()
    }
    fn clear(&self) -> Result<(), String> {
        self.memory.clear()
    }
    fn name(&self) -> &'static str {
        "delayed"
    }
}

fn delayed() -> (Arc<DelayedClipboard>, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started, waiting) = mpsc::channel();
    let (release, receiver) = mpsc::channel();
    (
        Arc::new(DelayedClipboard {
            memory: MemoryClipboard::new(),
            first: AtomicBool::new(true),
            started,
            release: Mutex::new(receiver),
            writes: Mutex::new(Vec::new()),
        }),
        waiting,
        release,
    )
}

#[test]
fn slow_copy_does_not_block_lock_and_late_writes_are_cleared() {
    let (backend, started, release) = delayed();
    let mut state = crate::app::tests::fixture_state();
    state.clipboard = Clipboard::with_background_backend(backend.clone());
    let before = Instant::now();
    state.clipboard.copy("first", None, before).unwrap();
    assert!(before.elapsed() < Duration::from_millis(100));
    started.recv_timeout(Duration::from_secs(1)).unwrap();
    // This queued write must never reach the backend after the lock.
    state.clipboard.copy("queued", None, Instant::now()).unwrap();
    let before = Instant::now();
    let effects = crate::app::update::update(&mut state, crate::app::Msg::LockSignal);
    assert!(before.elapsed() < Duration::from_millis(100));
    assert!(state.cache.is_empty());
    assert!(effects.iter().any(|e| matches!(
        e,
        crate::app::Effect::Backend { kind: crate::bw::service::CommandKind::Lock, .. }
    )));
    release.send(()).unwrap();
    state.clipboard.finish();
    state.clipboard.tick(Instant::now());
    assert!(state.clipboard.take_copy_result().is_none(), "a cancelled copy reported success");
    assert_eq!(backend.memory.contents(), None);
    assert_eq!(*backend.writes.lock().unwrap(), ["first"]);
}

#[test]
fn a_new_copy_after_lock_is_not_erased_by_a_stale_completion() {
    let (backend, started, release) = delayed();
    let mut clipboard = Clipboard::with_background_backend(backend.clone());
    clipboard.copy("old", None, Instant::now()).unwrap();
    started.recv_timeout(Duration::from_secs(1)).unwrap();
    clipboard.clear_ours();
    clipboard.copy("new", None, Instant::now()).unwrap();
    release.send(()).unwrap();
    clipboard.finish();
    clipboard.tick(Instant::now());
    assert_eq!(clipboard.take_copy_result(), Some(Ok(())));
    assert_eq!(backend.memory.contents().as_deref(), Some("new"));
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(Instant::now() < deadline, "clipboard worker did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn asynchronous_timeout_clears_without_ui_ticks() {
    let backend = Arc::new(MemoryClipboard::new());
    let mut clipboard = Clipboard::with_background_backend(backend.clone());
    clipboard.copy("temporary", Some(Duration::from_millis(80)), Instant::now()).unwrap();
    wait_until(|| backend.contents().is_some());
    wait_until(|| backend.contents().is_none());
    assert!(clipboard.tick(Instant::now()));
    assert!(!clipboard.has_pending());
    clipboard.finish();
}

#[test]
fn asynchronous_clear_leaves_an_unrelated_copy_alone() {
    let backend = Arc::new(MemoryClipboard::new());
    let mut clipboard = Clipboard::with_background_backend(backend.clone());
    clipboard.copy("ours", None, Instant::now()).unwrap();
    wait_until(|| backend.contents().as_deref() == Some("ours"));
    backend.set("browser link").unwrap();
    clipboard.clear_ours();
    clipboard.finish();
    assert_eq!(backend.contents().as_deref(), Some("browser link"));
}

#[derive(Debug)]
struct FailingClipboard;
impl ClipboardBackend for FailingClipboard {
    fn set(&self, _: &str) -> Result<(), String> {
        Err("clipboard unavailable".into())
    }
    fn read(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn clear(&self) -> Result<(), String> {
        Ok(())
    }
    fn name(&self) -> &'static str {
        "failing"
    }
}

#[test]
fn asynchronous_errors_reach_the_ui_without_reporting_copied() {
    let mut state = crate::app::tests::fixture_state();
    state.clipboard = Clipboard::with_background_backend(Arc::new(FailingClipboard));
    state.clipboard.copy("synthetic", None, Instant::now()).unwrap();
    state.clipboard.finish();
    crate::app::update::update(&mut state, crate::app::Msg::Tick);
    assert!(state.toasts.iter().any(|toast| toast.title == "Couldn't copy."));
    assert!(!state.clipboard.copying());
    assert!(!state.clipboard.has_pending());
}

#[test]
fn pipe_writes_and_reads_obey_the_tool_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("reader.sh");
    // A pipe-sized input would fill stdin before a wait-only timeout starts.
    std::fs::write(&script, "sleep 2\n").unwrap();
    let input = vec![b'x'; 1024 * 1024];
    for (input, capture) in [(Some(input.as_slice()), false), (None, true)] {
        let start = Instant::now();
        let result = run_tool("sh", &[script.to_str().unwrap()], input, capture);
        assert!(start.elapsed() < Duration::from_millis(1500));
        assert!(result.unwrap_err().contains("timed out"));
    }
    let echoed = run_tool("cat", &[], Some(b"synthetic\ntext"), true).unwrap();
    assert_eq!(echoed, b"synthetic\ntext");
}
