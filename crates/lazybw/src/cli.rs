//! Command-line interface: the TUI plus `keys`, `themes`, `config` and
//! `doctor`.

use crate::app::msg::{Msg, SuspendTask, Suspended};
use crate::app::{App, AppState, Effect, SessionPhase};
use crate::bw::{locate, service};
use crate::config::{Config, Paths};
use crate::theme::loader;
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// `lazybw` command line.
#[derive(Debug, Parser)]
#[command(name = "lazybw", version, about = "A keyboard-first terminal UI for the Bitwarden vault")]
pub struct Cli {
    /// Run a subcommand instead of the TUI.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the effective keymap.
    Keys {
        /// Only this context.
        #[arg(long)]
        context: Option<String>,
        /// Built-in defaults only, ignoring the user's config.
        #[arg(long)]
        defaults: bool,
        /// Validate the config and exit non-zero on errors.
        #[arg(long)]
        check: bool,
        /// Output format (only `text` for now).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// List the built-in themes.
    Themes {
        /// Print the palette of one theme.
        #[arg(long)]
        preview: Option<String>,
    },
    /// Inspect the configuration.
    Config {
        /// Print the config file path.
        #[arg(long)]
        path: bool,
        /// Print the default config.
        #[arg(long)]
        print_default: bool,
        /// Validate the config file.
        #[arg(long)]
        check: bool,
    },
    /// Print diagnostics: CLI, session, clipboard, terminal and paths.
    Doctor,
}

/// Entry point used by `main`.
pub fn run() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Keys { context, defaults, check, format }) => {
            run_keys(context.as_deref(), defaults, check, &format)
        }
        Some(Command::Themes { preview }) => run_themes(preview.as_deref()),
        Some(Command::Config { path, print_default, check }) => {
            run_config(path, print_default, check)
        }
        Some(Command::Doctor) => run_doctor(),
        None => run_tui(),
    }
}

fn load() -> (Config, Vec<String>, Option<String>) {
    let paths = Paths::resolve();
    match Config::load(&paths.config_file()) {
        Ok((config, warnings)) => {
            let errors = config.validate();
            if errors.is_empty() {
                (config, warnings, None)
            } else {
                (config, warnings, Some(errors.join("; ")))
            }
        }
        Err(err) => (Config::default(), Vec::new(), Some(err.summary())),
    }
}

fn run_keys(
    context: Option<&str>,
    defaults: bool,
    check: bool,
    format: &str,
) -> color_eyre::Result<()> {
    let (mut config, _warnings, config_error) = load();
    if defaults {
        config.keys = crate::config::KeysConfig::default();
    }
    let (keymap, diagnostics) = match crate::config::keymap::Keymap::build(&config.keys) {
        Ok(result) => result,
        Err(errors) => {
            for diagnostic in &errors {
                eprintln!("{}", diagnostic.line());
            }
            std::process::exit(1);
        }
    };
    if check {
        for diagnostic in &diagnostics {
            if diagnostic.severity != crate::config::keymap::Severity::Error {
                eprintln!("{}", diagnostic.line());
            }
        }
    }
    if let Some(error) = config_error {
        eprintln!("error: {error}");
        if check {
            std::process::exit(1);
        }
    }
    if check {
        if !diagnostics.iter().any(|d| d.severity == crate::config::keymap::Severity::Error) {
            println!("Keymap OK.");
            return Ok(());
        }
        std::process::exit(1);
    }
    let contexts: Vec<String> = match context {
        Some(context) => vec![context.to_owned()],
        None => keymap.flat.keys().cloned().collect(),
    };
    if format == "toml" {
        for context in &contexts {
            let mut rows = keymap.bindings_in(context);
            if rows.is_empty() {
                continue;
            }
            println!("[keys.{context}]");
            rows.sort_by(|a, b| {
                crate::config::keymap::display_keys(&a.0)
                    .cmp(&crate::config::keymap::display_keys(&b.0))
            });
            for (keys, call) in rows {
                println!(
                    "\"{}\" = \"{}\"",
                    crate::config::keymap::display_keys(&keys),
                    call.as_text()
                );
            }
            println!();
        }
        return Ok(());
    }
    println!("{:<16} {:<18} {:<28} {:<40} SOURCE", "CONTEXT", "KEYS", "ACTION", "DESCRIPTION");
    for context in &contexts {
        let mut rows = keymap.bindings_in(context);
        if rows.is_empty() {
            continue;
        }
        rows.sort_by(|a, b| {
            crate::config::keymap::display_keys(&a.0)
                .cmp(&crate::config::keymap::display_keys(&b.0))
        });
        for (keys, call) in rows {
            println!(
                "{:<16} {:<18} {:<28} {:<40} {:?}",
                context,
                crate::config::keymap::display_keys(&keys),
                call.id,
                crate::app::actions::description(&call.id),
                call.source
            );
        }
    }
    Ok(())
}

fn run_themes(preview: Option<&str>) -> color_eyre::Result<()> {
    let paths = Paths::resolve();
    let user = loader::load_user_themes(&paths.themes_dir());
    if let Some(name) = preview {
        match crate::theme::builtin::by_name(name) {
            Some(theme) => {
                println!("{} ({})", theme.name, theme.appearance.label());
                for (key, colour) in theme.palette.entries() {
                    println!("  {key:<16} {}", colour.hex().unwrap_or_else(|| "ansi".to_owned()));
                }
                return Ok(());
            }
            None => {
                let outcome = loader::load(name, &paths.themes_dir());
                if let Some(theme) = outcome.theme {
                    println!("{} ({})", theme.name, theme.appearance.label());
                    for (key, colour) in theme.palette.entries() {
                        println!(
                            "  {key:<16} {}",
                            colour.hex().unwrap_or_else(|| "ansi".to_owned())
                        );
                    }
                    return Ok(());
                }
                eprintln!("Theme not found: {name}");
                std::process::exit(1);
            }
        }
    }
    for name in crate::theme::builtin::names() {
        let theme = crate::theme::builtin::by_name(name)
            .expect("every name from builtin::names() is a built-in theme");
        println!("{:<20} {}", theme.name, theme.appearance.label());
    }
    for theme in &user.0 {
        println!("{:<20} {} (user)", theme.name, theme.appearance.label());
    }
    for warning in user.1 {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

fn run_config(path: bool, print_default: bool, check: bool) -> color_eyre::Result<()> {
    let paths = Paths::resolve();
    if path {
        println!("{}", paths.config_file().display());
        return Ok(());
    }
    if print_default {
        print!("{}", crate::config::DEFAULT_CONFIG);
        return Ok(());
    }
    let (config, warnings, error) = load();
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    if let Some(error) = &error {
        eprintln!("error: {error}");
        if check {
            std::process::exit(1);
        }
        return Ok(());
    }
    println!("Config OK.");
    println!("theme = {}", config.ui.theme);
    println!("config = {}", paths.config_file().display());
    println!("state = {}", paths.state_dir.display());
    Ok(())
}

fn run_doctor() -> color_eyre::Result<()> {
    let paths = Paths::resolve();
    let (config, _warnings, _error) = load();
    println!("lazybw doctor");
    println!("version      {}", env!("CARGO_PKG_VERSION"));
    println!("config       {}", paths.config_file().display());
    println!("state        {}", paths.state_dir.display());
    println!("log          {}", paths.log_file().display());
    println!("theme        {}", config.ui.theme);
    println!("icons        {}", config.ui.icons.label());
    match locate::locate(&config.bw.path) {
        Ok(location) => {
            println!("bw           {}", location.path.display());
            println!("bw version   {}", location.capabilities.version);
            println!(
                "capabilities archived={} new_item_types={} password_health={}",
                location.capabilities.archived_list,
                location.capabilities.new_item_types,
                location.capabilities.password_health
            );
            match futures_status(&location.path) {
                Ok(status) => {
                    println!("status       {}", status.status);
                    println!("account      {}", status.user_email.unwrap_or_default());
                    println!(
                        "server       {}",
                        status.server_url.unwrap_or_else(|| "bitwarden.com".to_owned())
                    );
                }
                Err(err) => println!("status       unknown ({err})"),
            }
        }
        Err(err) => println!("bw           {err}"),
    }
    let clipboard = crate::clipboard::Clipboard::system();
    println!("clipboard    {}", clipboard.backend_name());
    match crossterm::terminal::size() {
        Ok((cols, rows)) => println!("terminal     {cols}×{rows}"),
        Err(err) => println!("terminal     unknown ({err})"),
    }
    println!(
        "colour depth {}",
        if crate::theme::truecolor_available() { "truecolor" } else { "256" }
    );
    if std::env::var_os("TMUX").is_some() {
        println!("tmux hint    set -g extended-keys on   (needed for Ctrl-i vs Tab)");
    }
    println!(
        "nerd font    {}",
        if config.ui.icons == crate::config::IconMode::Nerdfont {
            "expected (ui.icons = nerdfont)"
        } else {
            "not required"
        }
    );
    Ok(())
}

fn futures_status(path: &std::path::Path) -> Result<crate::bw::models::Status, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(crate::bw::subprocess::status(path)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// TUI
// ---------------------------------------------------------------------------

fn run_tui() -> color_eyre::Result<()> {
    // `harden_process` restores the default SIGPIPE so `lazybw keys | head`
    // ends quietly. The TUI must not die on one: a write to a `bw` that has
    // just exited would kill it with the terminal still in raw mode. Ignored,
    // the write fails with EPIPE and is handled like any other error.
    #[expect(unsafe_code, reason = "signal(2) with constant arguments")]
    // SAFETY: setting a signal disposition to SIG_IGN has no memory effects.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    // Buffers left by a lazybw that crashed (an edit, a filtered import).
    let swept = crate::security::sweep_stale_temp_files();
    // Declared before the runtime so it is dropped after it: the log is
    // flushed once every task has stopped writing to it.
    let _log = crate::logging::init(&Paths::resolve().log_file());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "lazybw started");
    if swept > 0 {
        tracing::info!(swept, "removed temporary files left by an earlier crash");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(run_tui_async());
    match &result {
        Ok(()) => tracing::info!("lazybw exited"),
        Err(err) => tracing::error!(%err, "lazybw exited with an error"),
    }
    result
}

struct Startup {
    config: Config,
    theme: crate::theme::Theme,
    keymap: crate::config::keymap::Keymap,
    paths: Paths,
    warnings: Vec<String>,
    error: Option<String>,
}

fn startup() -> Startup {
    let paths = Paths::resolve();
    let (config, mut warnings, mut error) = load();
    let theme_outcome = loader::load(&config.ui.theme, &paths.themes_dir());
    warnings.extend(theme_outcome.warnings);
    let theme = theme_outcome.theme.unwrap_or_else(|| {
        crate::theme::builtin::by_name("tokyonight-night")
            .expect("tokyonight-night is a built-in theme")
    });
    let (keymap, diagnostics) = match crate::config::keymap::Keymap::build(&config.keys) {
        Ok(result) => result,
        Err(errors) => {
            let message = errors.iter().map(|d| d.line()).collect::<Vec<_>>().join("\n");
            error = Some(message);
            (
                crate::config::keymap::Keymap::build(&crate::config::KeysConfig::default())
                    .map(|r| r.0)
                    .unwrap_or_default(),
                Vec::new(),
            )
        }
    };
    // Only report what the user's own overrides introduced: the defaults
    // intentionally shadow globals (Esc in visual mode, …) and are not news.
    // The baseline is only needed when there are diagnostics to filter.
    if !diagnostics.is_empty() {
        let baseline: std::collections::BTreeSet<String> =
            crate::config::keymap::Keymap::build(&crate::config::KeysConfig::default())
                .map(|(_, d)| d.iter().map(|d| d.line()).collect())
                .unwrap_or_default();
        for diagnostic in diagnostics {
            if diagnostic.severity == crate::config::keymap::Severity::Warning
                && !baseline.contains(&diagnostic.line())
            {
                warnings.push(diagnostic.line());
            }
        }
    }
    Startup { config, theme, keymap, paths, warnings, error }
}

async fn run_tui_async() -> color_eyre::Result<()> {
    let Startup { config, mut theme, keymap, paths, warnings, error } = startup();
    theme.transparent = config.ui.transparent;
    theme.resolve(crate::theme::truecolor_available(), crate::theme::no_color());
    let mut state = AppState::new(config.clone(), theme, keymap, paths.clone());
    crate::app::load_command_history(&mut state);
    state.keyboard_enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    for warning in warnings {
        state.message(crate::app::state::MessageLevel::Warn, warning);
    }
    if let Some(error) = error {
        state.enter_session(SessionPhase::Failed(Box::new(crate::app::state::ErrorScreen {
            title: "Configuration error".to_owned(),
            lines: error.lines().map(str::to_owned).collect(),
            detail: Vec::new(),
            actions: "e edit config   r retry   q quit".to_owned(),
            retry: true,
            open_file: Some(paths.config_file()),
        })));
    }

    let mut app = App::new(state);
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Msg>(512);

    init_terminal(&config)?;
    // From here on the terminal is ours: whatever ends this function (a
    // failed draw, a signal handler that cannot be installed) gives it back.
    let terminal_guard = TerminalGuard;
    let input_thread = spawn_input_thread(input_tx.clone());
    let mut sigusr1 =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    let mut terminal = ratatui::init();
    silence_stderr();
    // Give the whole terminal back before a panic report is printed: stderr,
    // and the modes ratatui's own hook leaves on (mouse capture, bracketed
    // paste, focus events, the keyboard protocol), which would otherwise
    // follow the user into their shell.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        report(info);
    }));
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let mut redraw = tokio::time::interval(Duration::from_millis(50));
    // The redraw branch sits idle between animations; when the next one
    // starts, skip the ticks missed meanwhile instead of drawing them all in
    // a burst.
    redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Show the starting screen before locating the CLI. `bw --version` starts
    // Node and takes hundreds of milliseconds on a cold cache; the user should
    // see the interface, not a blank terminal.
    if let Ok(size) = terminal.size() {
        app.state.size = (size.width, size.height);
    }
    redraw_now(&mut app, &mut terminal)?;

    let (location, locate_error) =
        match locate::locate_cached(&config.bw.path, Some(&paths.bw_version_file())) {
            Ok(location) => (Some(location), None),
            Err(err) => (None, Some(err)),
        };
    match (&location, &locate_error) {
        (Some(location), _) => tracing::info!(
            path = %location.path.display(),
            version = %location.capabilities.version,
            transport = ?config.bw.transport,
            "found bw"
        ),
        (None, Some(err)) => tracing::warn!(%err, "bw cannot be used"),
        (None, None) => {}
    }
    // A configuration error is shown first: fixing config.toml (which may
    // well be what sets bw.path) comes before installing the CLI.
    if let Some(error) = locate_error.filter(|_| app.state.error().is_none()) {
        app.state.enter_session(SessionPhase::Failed(Box::new(locate_error_screen(&error))));
    }
    if let Some(location) = &location {
        app.state.bw_path = Some(location.path.clone());
        app.state.capabilities = Some(location.capabilities.clone());
        let backend_config = service::BackendConfig {
            bw_path: location.path.clone(),
            transport: config.bw.transport,
            runtime_dir: crate::security::secure_temp_dir()
                .unwrap_or_else(|_| PathBuf::from("/tmp")),
            inherit_session: config.bw.inherit_session,
            capabilities: location.capabilities.clone(),
        };
        let (backend, mut events) = service::spawn(backend_config);
        app.backend = Some(backend);
        let forward = input_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if forward.send(Msg::Backend(event)).await.is_err() {
                    break;
                }
            }
        });
    }
    redraw_now(&mut app, &mut terminal)?;

    let start_effects = app.start();
    let mut pending = app.run_effects(start_effects).await;
    let mut result: color_eyre::Result<()> = Ok(());

    'outer: loop {
        let key_deadline = app.state.key_deadline();
        tokio::select! {
            maybe = input_rx.recv() => {
                let Some(msg) = maybe else { break };
                // Handle whatever input is already queued (a paste, a held
                // key) before drawing, so a burst costs one frame, not one
                // per key; cap it so the screen still updates under a flood.
                let mut next = Some(msg);
                let mut handled = 0;
                while let Some(msg) = next.take() {
                    let mut effects = app.handle(msg).await;
                    effects.extend(std::mem::take(&mut pending));
                    if let Some(quit) = handle_suspended_effects(&mut app, &mut terminal, &config, &input_thread, &mut sigusr1, effects).await
                        && quit {
                            break 'outer;
                        }
                    handled += 1;
                    if handled < 64 && !app.state.quit {
                        next = input_rx.try_recv().ok();
                    }
                }
                // Draw now rather than waiting for the redraw timer: this is
                // the keystroke-to-screen latency the user actually feels.
                redraw_now(&mut app, &mut terminal)?;
            }
            _ = ticker.tick() => {
                let effects = app.handle(Msg::Tick).await;
                let mut effects = effects;
                effects.extend(pending);
                pending = Vec::new();
                if let Some(quit) = handle_suspended_effects(&mut app, &mut terminal, &config, &input_thread, &mut sigusr1, effects).await
                    && quit {
                        break 'outer;
                    }
                redraw_now(&mut app, &mut terminal)?;
            }
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(
                key_deadline.unwrap_or_else(std::time::Instant::now),
            )), if key_deadline.is_some() => {
                let mut effects = app.handle(Msg::KeyTimer).await;
                effects.extend(std::mem::take(&mut pending));
                if let Some(quit) = handle_suspended_effects(&mut app, &mut terminal, &config, &input_thread, &mut sigusr1, effects).await
                    && quit {
                        break 'outer;
                    }
                redraw_now(&mut app, &mut terminal)?;
            }
            _ = redraw.tick(), if app.state.dirty => {
                redraw_now(&mut app, &mut terminal)?;
            }
            _ = sigusr1.recv() => {
                // Like every other branch: effects the lock produces (the
                // request that locks bw serve) must run, not be dropped.
                let mut effects = app.handle(Msg::LockSignal).await;
                effects.extend(std::mem::take(&mut pending));
                if let Some(quit) = handle_suspended_effects(&mut app, &mut terminal, &config, &input_thread, &mut sigusr1, effects).await
                    && quit {
                        break 'outer;
                    }
                redraw_now(&mut app, &mut terminal)?;
            }
            _ = sigterm.recv() => {
                break 'outer;
            }
            _ = sighup.recv() => {
                break 'outer;
            }
        }
        if app.state.quit {
            break;
        }
    }
    if config.security.clear_clipboard_on_exit {
        app.state.clipboard.clear_ours();
    }
    // Close the clipboard (and the backend) while stderr is still silenced.
    drop(app);
    let _ = &mut result;
    drop(terminal_guard);
    input_thread.stop();
    result
}

/// Draws a frame when the state changed. Called right after handling input so
/// the keystroke reaches the screen in one frame time, not after the 50 ms
/// animation tick.
fn redraw_now(app: &mut App, terminal: &mut ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    if !app.state.dirty {
        return Ok(());
    }
    if std::mem::take(&mut app.state.full_redraw) {
        terminal.clear()?;
    }
    terminal.draw(|frame| crate::ui::draw(frame, &app.state))?;
    // The 50 ms redraw timer only fires while the state is dirty, so stay
    // dirty until the animation on screen settles. Otherwise an effect only
    // moves when something else draws a frame (a key, an answer from bw, the
    // 1 s tick), and the 250 ms sweep after unlocking takes two seconds.
    app.state.dirty = app.state.animating();
    Ok(())
}

/// Handles deferred effects, including terminal suspension. Returns
/// `Some(true)` when the app should quit.
async fn handle_suspended_effects(
    app: &mut App,
    terminal: &mut ratatui::DefaultTerminal,
    config: &Config,
    input: &InputThread,
    lock_signal: &mut tokio::signal::unix::Signal,
    effects: Vec<Effect>,
) -> Option<bool> {
    for effect in effects {
        match effect {
            Effect::Suspend(task) => {
                // Stop reading the terminal first: otherwise this process
                // keeps consuming keys meant for $EDITOR or `bw login`
                // (the editor seems frozen, and a typed master password
                // would land in lazybw's key queue).
                input.pause();
                restore_terminal();
                // The child runs on a blocking thread so this loop keeps
                // clearing the clipboard on time and still honours SIGUSR1
                // while $EDITOR or `bw login` is open.
                let editor = config.editor.command.clone();
                let bw = app.state.bw_path.clone();
                let mut child =
                    tokio::task::spawn_blocking(move || run_suspended(task, &editor, bw));
                let mut clock = tokio::time::interval(Duration::from_secs(1));
                let mut lock_requested = false;
                let result = loop {
                    tokio::select! {
                        joined = &mut child => break joined.unwrap_or(Suspended::Cancelled),
                        _ = clock.tick() => {
                            app.state.clipboard.tick(std::time::Instant::now());
                        }
                        _ = lock_signal.recv() => lock_requested = true,
                    }
                };
                *terminal = ratatui::init();
                silence_stderr();
                // Everything `restore_terminal` turned off, not only the
                // mouse and the keyboard protocol: without bracketed paste a
                // pasted text arrives as keys and runs as commands, and
                // without focus events `hide_on_focus_lost` stops working.
                let _ = init_terminal(config);
                input.resume();
                // Time in the editor is activity: the idle lock must not
                // fire the moment it closes and throw the edit away.
                app.state.touch();
                let discarded = lock_requested
                    && matches!(
                        result,
                        Suspended::Notes(_) | Suspended::ItemJson(_) | Suspended::SshPrivateKey(_)
                    );
                let msg = if lock_requested {
                    // Text from the editor must not reach a vault that was
                    // locked while it was open; lock instead.
                    Msg::LockSignal
                } else {
                    Msg::Suspended(result)
                };
                let effects = app.handle(msg).await;
                if discarded {
                    app.state.message(
                        crate::app::state::MessageLevel::Warn,
                        "The vault was locked while the editor was open; those changes were discarded.",
                    );
                }
                if effects.iter().any(|e| matches!(e, Effect::Quit)) {
                    return Some(true);
                }
                let _ = app.run_effects(effects).await;
            }
            Effect::Quit => return Some(true),
            other => {
                let remaining = app.run_effects(vec![other]).await;
                if remaining.iter().any(|e| matches!(e, Effect::Quit)) {
                    return Some(true);
                }
            }
        }
    }
    None
}

/// The thread that reads terminal events, with a pause handshake so a
/// suspended child (`$EDITOR`, `bw login`) owns the keyboard alone.
struct InputThread {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    paused: std::sync::Arc<std::sync::atomic::AtomicBool>,
    idle: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl InputThread {
    /// Stops reading and waits until the thread is out of `poll`/`read`.
    fn pause(&self) {
        use std::sync::atomic::Ordering;
        self.paused.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while !self.idle.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Reads events again.
    fn resume(&self) {
        use std::sync::atomic::Ordering;
        self.idle.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Ends the thread.
    fn stop(&self) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

fn spawn_input_thread(tx: tokio::sync::mpsc::Sender<Msg>) -> InputThread {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let control = InputThread {
        running: Arc::new(AtomicBool::new(true)),
        paused: Arc::new(AtomicBool::new(false)),
        idle: Arc::new(AtomicBool::new(false)),
    };
    let flag = Arc::clone(&control.running);
    let paused = Arc::clone(&control.paused);
    let idle = Arc::clone(&control.idle);
    std::thread::spawn(move || {
        use crossterm::event::{self, Event};
        while flag.load(Ordering::Relaxed) {
            if paused.load(Ordering::SeqCst) {
                idle.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            // Short, so `pause` (before $EDITOR or `bw login` takes the
            // terminal) never waits long for this thread to let go.
            match event::poll(Duration::from_millis(25)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => {
                        if tx.blocking_send(Msg::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Mouse(mouse)) => {
                        if tx.blocking_send(Msg::Mouse(mouse)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if tx.blocking_send(Msg::Paste(text)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Resize(cols, rows)) => {
                        if tx.blocking_send(Msg::Resize(cols, rows)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::FocusGained) => {
                        let _ = tx.blocking_send(Msg::FocusGained);
                    }
                    Ok(Event::FocusLost) => {
                        let _ = tx.blocking_send(Msg::FocusLost);
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    control
}

fn init_terminal(config: &Config) -> color_eyre::Result<()> {
    let mut out = std::io::stdout();
    crossterm::execute!(
        out,
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableFocusChange
    )?;
    let _ = init_terminal_extras(config);
    Ok(())
}

fn init_terminal_extras(config: &Config) -> color_eyre::Result<()> {
    let mut out = std::io::stdout();
    if crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
        let _ = crossterm::execute!(
            out,
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        );
    }
    if config.ui.mouse {
        let _ = crossterm::execute!(out, crossterm::event::EnableMouseCapture);
    }
    Ok(())
}

/// fd 2 as it was before the TUI took over the terminal.
static SAVED_STDERR: std::sync::Mutex<Option<std::os::fd::OwnedFd>> = std::sync::Mutex::new(None);

/// Points fd 2 at `/dev/null` while the TUI owns the screen. A library that
/// writes to stderr (arboard warns there, for one) would otherwise print over
/// the alternate screen, and ratatui only repaints cells it changed, so the
/// stray text stays until something redraws over it. Skipped when stderr is
/// not a terminal: a redirect to a file is the user's choice.
fn silence_stderr() {
    use std::io::IsTerminal;
    // Recovering a poisoned lock is safe: it holds one saved descriptor,
    // and stderr must be restorable even on the way out of a panic.
    let mut saved = SAVED_STDERR.lock().unwrap_or_else(|e| e.into_inner());
    if saved.is_some() || !std::io::stderr().is_terminal() {
        return;
    }
    let Ok(copy) = rustix::io::dup(rustix::stdio::stderr()) else { return };
    let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") else { return };
    if rustix::stdio::dup2_stderr(&null).is_ok() {
        *saved = Some(copy);
    }
}

/// Undoes [`silence_stderr`].
fn restore_stderr() {
    let saved = SAVED_STDERR.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(fd) = saved {
        let _ = rustix::stdio::dup2_stderr(&fd);
    }
}

/// Gives the terminal back when dropped, so every way out of the TUI does.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Undoes [`init_terminal`] and `ratatui::init`. Safe to call more than
/// once: the panic hook and [`TerminalGuard`] may both get to it.
fn restore_terminal() {
    restore_stderr();
    let mut out = std::io::stdout();
    let _ = crossterm::execute!(
        out,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableFocusChange,
        crossterm::event::DisableBracketedPaste,
        crossterm::event::PopKeyboardEnhancementFlags,
        crossterm::terminal::LeaveAlternateScreen
    );
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = out.flush();
}

fn run_suspended(task: SuspendTask, editor: &str, bw: Option<PathBuf>) -> Suspended {
    match task {
        SuspendTask::Login { sso } => {
            let kind = if sso {
                crate::bw::subprocess::InteractiveLogin::Sso
            } else {
                crate::bw::subprocess::InteractiveLogin::Password
            };
            println!(
                "lazybw › Running \"bw login\". Follow the prompts; lazybw resumes when it finishes."
            );
            // The `bw` found at start-up; looking again would run
            // `bw --version`, a second of Node, before the prompt appears.
            let Some(path) = bw else {
                println!("lazybw › Login failed: bw was not found. Press Enter to return.");
                let _ = std::io::stdin().read_line(&mut String::new());
                return Suspended::Login(None);
            };
            let mut child = crate::bw::subprocess::interactive_login_command(&path, kind).spawn();
            match child.as_mut() {
                Ok(child) => {
                    let status = child.wait();
                    let mut key = String::new();
                    if let Some(stdout) = child.stdout.take() {
                        use std::io::Read;
                        let mut reader = std::io::BufReader::new(stdout);
                        let _ = reader.read_to_string(&mut key);
                    }
                    let success =
                        status.map(|s| s.success()).unwrap_or(false) && !key.trim().is_empty();
                    if success {
                        return Suspended::Login(Some(secrecy::SecretString::from(
                            key.trim().to_owned(),
                        )));
                    }
                    println!("lazybw › Login failed. Press Enter to return.");
                    let _ = std::io::stdin().read_line(&mut String::new());
                    Suspended::Login(None)
                }
                Err(err) => {
                    println!("lazybw › Couldn't start bw login: {err}. Press Enter to return.");
                    let _ = std::io::stdin().read_line(&mut String::new());
                    Suspended::LoginFailed
                }
            }
        }
        SuspendTask::Editor { target } => {
            let (text, json) = match target {
                crate::app::msg::EditorTarget::Notes(text) => (text, false),
                crate::app::msg::EditorTarget::Json(text) => (text, true),
                crate::app::msg::EditorTarget::SshPrivateKey(text) => {
                    return match crate::external_editor::edit_text(text.expose(), editor) {
                        Ok(edited) => {
                            Suspended::SshPrivateKey(crate::bw::sensitive::Sensitive::new(edited))
                        }
                        Err(err) => Suspended::Failed(err.to_string()),
                    };
                }
                crate::app::msg::EditorTarget::Config(path) => {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    match crate::external_editor::edit_text(&text, editor) {
                        Ok(edited) => {
                            let _ = std::fs::write(&path, edited);
                        }
                        Err(err) => return Suspended::Failed(err.to_string()),
                    }
                    return Suspended::Cancelled;
                }
            };
            match crate::external_editor::edit_text(text.expose(), editor) {
                Ok(edited) => {
                    let edited = crate::bw::sensitive::Sensitive::new(edited);
                    if json { Suspended::ItemJson(edited) } else { Suspended::Notes(edited) }
                }
                Err(err) => Suspended::Failed(err.to_string()),
            }
        }
        SuspendTask::LoginFailed => Suspended::LoginFailed,
    }
}

/// The start-up screen for a CLI that could not be found or used. lazybw
/// looks for `bw` once, at start-up, so none of them offers a retry.
fn locate_error_screen(error: &locate::LocateError) -> crate::app::state::ErrorScreen {
    use locate::LocateError;
    let needs =
        format!("lazybw needs the official Bitwarden CLI (bw) {} or newer.", locate::MIN_VERSION);
    let (title, mut lines) = match error {
        LocateError::NotFound { configured, env } => {
            let shown = |path: &Option<PathBuf>| {
                path.as_ref().map_or_else(|| "unset".to_owned(), |p| p.display().to_string())
            };
            (
                "Bitwarden CLI not found",
                vec![
                    needs,
                    String::new(),
                    "Install it with one of:".to_owned(),
                    "  npm install -g @bitwarden/cli".to_owned(),
                    "  your distribution's package (bitwarden-cli)".to_owned(),
                    String::new(),
                    format!(
                        "Searched: bw.path ({}) · $LAZYBW_BW ({}) · $PATH",
                        shown(configured),
                        shown(env)
                    ),
                ],
            )
        }
        LocateError::TooOld(version) => (
            "Bitwarden CLI too old",
            vec![
                format!("Found bw {version}. lazybw needs {} or newer.", locate::MIN_VERSION),
                String::new(),
                "Update it with one of:".to_owned(),
                "  npm update -g @bitwarden/cli".to_owned(),
                "  your distribution's package (bitwarden-cli)".to_owned(),
            ],
        ),
        LocateError::NotExecutable { .. } | LocateError::UnparseableVersion(_) => {
            ("Bitwarden CLI not usable", vec![needs, String::new(), error.to_string()])
        }
    };
    lines.extend([String::new(), "Then start lazybw again.".to_owned()]);
    crate::app::state::ErrorScreen {
        title: title.to_owned(),
        lines,
        detail: Vec::new(),
        actions: "q quit".to_owned(),
        retry: false,
        open_file: None,
    }
}

#[cfg(test)]
mod tests {
    use super::locate_error_screen;
    use crate::bw::locate::LocateError;
    use std::path::PathBuf;

    #[test]
    fn a_missing_cli_lists_where_lazybw_looked() {
        let screen = locate_error_screen(&LocateError::NotFound {
            configured: Some(PathBuf::from("/opt/bw/bin/bw")),
            env: None,
        });
        assert_eq!(screen.title, "Bitwarden CLI not found");
        assert!(
            screen.lines.contains(
                &"Searched: bw.path (/opt/bw/bin/bw) · $LAZYBW_BW (unset) · $PATH".to_owned()
            ),
            "{:?}",
            screen.lines
        );
        assert!(!screen.retry);
    }

    #[test]
    fn an_old_cli_asks_for_an_update_not_an_install() {
        let screen = locate_error_screen(&LocateError::TooOld(semver::Version::new(2025, 6, 0)));
        assert_eq!(screen.title, "Bitwarden CLI too old");
        let text = screen.lines.join("\n");
        assert!(text.contains("Found bw 2025.6.0. lazybw needs"), "{text}");
        assert!(text.contains("npm update -g @bitwarden/cli"), "{text}");
        assert!(!text.contains("Searched:") && !text.contains("npm install"), "{text}");
    }

    #[test]
    fn a_cli_that_cannot_run_shows_the_reason() {
        let screen = locate_error_screen(&LocateError::NotExecutable {
            path: PathBuf::from("/usr/bin/bw"),
            message: "Permission denied".to_owned(),
        });
        assert_eq!(screen.title, "Bitwarden CLI not usable");
        assert!(screen.lines.contains(&"Could not run /usr/bin/bw: Permission denied".to_owned()));
        assert_eq!(screen.lines.last().map(String::as_str), Some("Then start lazybw again."));
    }
}
