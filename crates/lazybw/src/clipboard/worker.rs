//! Serial clipboard I/O away from the terminal loop. A generation invalidates
//! queued copies on lock and prevents late completions from restoring UI state.

use super::{Clipboard, ClipboardBackend};
use crate::bw::sensitive::Sensitive;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub(super) struct Worker {
    sender: Option<mpsc::Sender<Command>>,
    events: Option<mpsc::Receiver<Event>>,
    thread: Option<std::thread::JoinHandle<()>>,
    generation: Arc<AtomicU64>,
    copying: bool,
    deadline: Option<Instant>,
    result: Option<Result<(), String>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardWorker").field("copying", &self.copying).finish_non_exhaustive()
    }
}

enum Command {
    Copy { generation: u64, text: Sensitive, timeout: Option<Duration> },
    Clear { generation: u64, unconditional: bool },
    Stop,
}

struct Event {
    generation: u64,
    kind: EventKind,
}

enum EventKind {
    Copied(Option<Instant>),
    Failed(String),
    Cleared,
}

impl Worker {
    pub(super) fn new() -> Self {
        Self {
            sender: None,
            events: None,
            thread: None,
            generation: Arc::new(AtomicU64::new(0)),
            copying: false,
            deadline: None,
            result: None,
        }
    }

    pub(super) fn start(&mut self, backend: Arc<dyn ClipboardBackend>) -> Result<(), String> {
        if self.sender.is_some() {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let generation = Arc::clone(&self.generation);
        let thread = std::thread::Builder::new()
            .name("lazybw-clipboard".into())
            .spawn(move || run(backend, rx, events, generation))
            .map_err(|e| format!("could not start clipboard worker: {e}"))?;
        self.sender = Some(tx);
        self.events = Some(receiver);
        self.thread = Some(thread);
        Ok(())
    }

    pub(super) fn copy(
        &mut self,
        backend: Arc<dyn ClipboardBackend>,
        text: &str,
        timeout: Option<Duration>,
    ) -> Result<(), String> {
        self.start(backend)?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.result = None;
        self.deadline = None;
        self.sender
            .as_ref()
            .ok_or("clipboard worker stopped")?
            .send(Command::Copy { generation, text: Sensitive::new(text), timeout })
            .map_err(|_| "clipboard worker stopped".to_owned())?;
        self.copying = true;
        Ok(())
    }

    pub(super) fn clear(&mut self, unconditional: bool) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.copying = false;
        self.deadline = None;
        self.result = None;
        if let Some(sender) = &self.sender {
            let _ = sender.send(Command::Clear { generation, unconditional });
        }
    }

    pub(super) fn tick(&mut self) -> bool {
        let mut cleared = false;
        if let Some(events) = &self.events {
            while let Ok(event) = events.try_recv() {
                if event.generation != self.generation.load(Ordering::SeqCst) {
                    continue;
                }
                match event.kind {
                    EventKind::Copied(deadline) => {
                        self.copying = false;
                        self.deadline = deadline;
                        self.result = Some(Ok(()));
                    }
                    EventKind::Failed(error) => {
                        self.copying = false;
                        self.deadline = None;
                        self.result = Some(Err(error));
                    }
                    EventKind::Cleared => {
                        self.deadline = None;
                        // A very short timeout may expire before the UI polls.
                        self.result = None;
                        cleared = true;
                    }
                }
            }
        }
        cleared
    }

    pub(super) fn copying(&self) -> bool {
        self.copying
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub(super) fn take_result(&mut self) -> Option<Result<(), String>> {
        self.result.take()
    }

    /// Only used while exiting, after the caller queued its cleanup policy.
    /// Normal operation and locking never join a potentially stuck worker.
    pub(super) fn finish(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Command::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Command::Stop);
        }
    }
}

fn run(
    backend: Arc<dyn ClipboardBackend>,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
    generation: Arc<AtomicU64>,
) {
    let mut clipboard = Clipboard::with_backend(backend);
    let mut active = 0;
    loop {
        let command = match clipboard.pending.as_ref() {
            Some(pending) => {
                commands.recv_timeout(pending.deadline.saturating_duration_since(Instant::now()))
            }
            None => commands.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match command {
            Ok(Command::Copy { generation: request, text, timeout }) => {
                if request != generation.load(Ordering::SeqCst) {
                    continue;
                }
                let result = clipboard.copy(text.expose(), timeout, Instant::now());
                // set() may have been in progress when the vault was locked.
                // It is ordered before the clear and no newer copy can have
                // reached the backend yet, so clear only our completed write.
                if request != generation.load(Ordering::SeqCst) {
                    clipboard.clear_ours();
                    continue;
                }
                active = request;
                let kind = match result {
                    Ok(()) => {
                        // Count from completion, not from a potentially slow write.
                        if let (Some(pending), Some(timeout)) = (&mut clipboard.pending, timeout) {
                            pending.deadline = Instant::now() + timeout;
                        }
                        EventKind::Copied(clipboard.pending.as_ref().map(|p| p.deadline))
                    }
                    Err(error) => EventKind::Failed(error),
                };
                let _ = events.send(Event { generation: request, kind });
            }
            Ok(Command::Clear { generation: request, unconditional }) => {
                active = request;
                if unconditional {
                    clipboard.clear_now();
                } else {
                    clipboard.clear_ours();
                }
                let _ = events.send(Event { generation: request, kind: EventKind::Cleared });
            }
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if clipboard.tick(Instant::now()) {
                    let _ = events.send(Event { generation: active, kind: EventKind::Cleared });
                }
            }
        }
    }
}
