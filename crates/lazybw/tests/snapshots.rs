//! Snapshot tests of every screen, rendered with ratatui's `TestBackend`.
//!
//! Text and the style pattern are snapshotted: each row's foreground,
//! background and modifier sequence is hashed so a colour regression fails
//! even when the characters are unchanged.

use lazybw::app::state::{
    AppState, CaptureState, CmdlineState, ConfirmState, DetailState, EditorState, ExportState,
    FormKey, FormKind, FormRow, HistoryState, ImportPreview, ImportState, ImportStep, InfoState,
    MessageLevel, Overlay, PickerEntry, PickerKind, PickerState, QuickOpenState, RepromptState,
    Screen, SessionPhase, Tab, WhichKeyState,
};
use lazybw::bw::models::{Folder, Item, ItemType, Login, LoginUri};
use lazybw::bw::sensitive::Sensitive;
use lazybw::config::Paths;
use lazybw::theme::builtin;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::hash::{Hash, Hasher};

fn state(theme: &str) -> AppState {
    let config = lazybw::config::Config::default();
    let mut theme = builtin::by_name(theme).expect("theme");
    theme.resolve(true, false);
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let mut state = AppState::new(
        config,
        theme,
        keymap,
        Paths {
            config_dir: "/tmp/lazybw-test/config".into(),
            state_dir: "/tmp/lazybw-test/state".into(),
        },
    );
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.config.ui.animations = false;
    state.fx.borrow_mut().set_enabled(false);
    state.now = lazybw::util::time::test_now();
    state.size = (140, 40);
    state.screen = Screen::Vault;
    state.session = SessionPhase::Ready;
    state.email = Some("you@example.com".to_owned());
    state.server = Some("vault.bitwarden.eu".to_owned());
    state.last_sync = Some(state.now - chrono::Duration::minutes(2));
    state.cache.load(items(), folders(), vec![], vec![]);
    state
}

fn items() -> Vec<Item> {
    let mut github = Item::new(ItemType::Login);
    github.id = Some("id-github".into());
    github.name = "GitHub".into();
    github.favorite = true;
    github.folder_id = Some("folder-personal".into());
    github.creation_date = Some("2023-03-12T09:30:00Z".into());
    github.revision_date = Some("2026-10-27T09:30:00Z".into());
    github.login = Some(Login {
        username: Some(Sensitive::new("dev-example")),
        password: Some(Sensitive::new("k7#Qm!vR2x@Lp9$Wn4eT")),
        totp: Some(Sensitive::new("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")),
        uris: Some(vec![LoginUri::new("https://github.com")]),
        ..Default::default()
    });
    github.fields = Some(vec![
        lazybw::bw::models::Field {
            name: Some("Recovery token".into()),
            value: Some(Sensitive::new("abc123")),
            kind: 1,
            ..Default::default()
        },
        lazybw::bw::models::Field {
            name: Some("2FA enabled".into()),
            value: Some(Sensitive::new("true")),
            kind: 2,
            ..Default::default()
        },
    ]);
    github.notes = Some(Sensitive::new("Recovery codes are in the safe, drawer 2."));
    let mut gmail = Item::new(ItemType::Login);
    gmail.id = Some("id-gmail".into());
    gmail.name = "Gmail".into();
    gmail.creation_date = Some("2024-01-01T00:00:00Z".into());
    gmail.revision_date = Some("2026-10-20T09:30:00Z".into());
    gmail.login = Some(Login {
        username: Some(Sensitive::new("personal@example.com")),
        password: Some(Sensitive::new("another-password")),
        ..Default::default()
    });
    let mut card = Item::new(ItemType::Card);
    card.id = Some("id-card".into());
    card.name = "Personal visa".into();
    card.creation_date = Some("2023-01-01T00:00:00Z".into());
    card.revision_date = Some("2026-07-20T00:00:00Z".into());
    card.card = Some(lazybw::bw::models::Card {
        cardholder_name: Some("Jane Example".into()),
        brand: Some("Visa".into()),
        number: Some(Sensitive::new("4242424242424242")),
        exp_month: Some("4".into()),
        exp_year: Some("2028".into()),
        ..Default::default()
    });
    let mut note = Item::new(ItemType::SecureNote);
    note.id = Some("id-note".into());
    note.name = "Router notes".into();
    note.notes = Some(Sensitive::new("admin / hunter2"));
    let mut ssh = Item::new(ItemType::SshKey);
    ssh.id = Some("id-ssh".into());
    ssh.name = "Hetzner deploy key".into();
    ssh.ssh_key = Some(lazybw::bw::models::SshKey {
        public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAQx9 deploy".into(),
        key_fingerprint: "SHA256:Qx9hY2f7A".into(),
        ..Default::default()
    });
    let mut identity = Item::new(ItemType::Identity);
    identity.id = Some("id-identity".into());
    identity.name = "Jane Example".into();
    identity.identity = Some(lazybw::bw::models::Identity {
        title: Some("Ms".into()),
        first_name: Some("Jane".into()),
        last_name: Some("Example".into()),
        email: Some("jane@example.com".into()),
        phone: Some("+351 900 000 000".into()),
        city: Some("Lisbon".into()),
        country: Some("Portugal".into()),
        ..Default::default()
    });
    let mut other = Item::new(ItemType::Other(6));
    other.id = Some("id-other".into());
    other.name = "Bank account".into();
    other.extra.insert(
        "bankAccount".into(),
        serde_json::json!({"bankName": "Example Bank", "accountNumber": "PT50 0000 0000 0000"}),
    );
    vec![github, gmail, card, note, ssh, identity, other]
}

fn folders() -> Vec<Folder> {
    vec![
        Folder {
            id: Some("folder-personal".into()),
            name: "Personal".into(),
            ..Default::default()
        },
        Folder {
            id: Some("folder-banking".into()),
            name: "Personal/Banking".into(),
            ..Default::default()
        },
        Folder { id: Some("folder-work".into()), name: "Work".into(), ..Default::default() },
    ]
}

fn render(state: &AppState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| lazybw::ui::draw(frame, state)).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out.push_str("styles:\n");
    for y in 0..height {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for x in 0..width {
            let style = buffer[(x, y)].style();
            format!("{:?}{:?}{:?}", style.fg, style.bg, style.add_modifier).hash(&mut hasher);
        }
        out.push_str(&format!("{y:02}:{:016x}\n", hasher.finish()));
    }
    out
}

#[test]
fn starting_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Starting;
    state.session = SessionPhase::Starting;
    state.bw_path = Some("/usr/bin/bw".into());
    state.capabilities =
        Some(lazybw::bw::locate::Capabilities::from_version(semver::Version::new(2026, 10, 1)));
    insta::assert_snapshot!("starting_120x36", render(&state, 120, 36));
    // The master password typed ahead shows masked.
    state.unlock.password.push_str("secret");
    insta::assert_snapshot!("starting_typed_ahead_120x36", render(&state, 120, 36));
}

#[test]
fn vault_screen_wide() {
    let mut state = state("tokyonight-night");
    state.focus = lazybw::app::state::Focus::Detail;
    state.detail = DetailState::default();
    insta::assert_snapshot!("vault_160x45", render(&state, 160, 45));
}

#[test]
fn vault_screen_medium() {
    let state = state("tokyonight-night");
    insta::assert_snapshot!("vault_120x36", render(&state, 120, 36));
}

#[test]
fn vault_screen_narrow() {
    let state = state("tokyonight-night");
    insta::assert_snapshot!("vault_90x30", render(&state, 90, 30));
}

#[test]
fn vault_screen_compact() {
    let state = state("tokyonight-night");
    insta::assert_snapshot!("vault_80x24", render(&state, 80, 24));
}

#[test]
fn vault_in_every_theme() {
    for theme in builtin::names() {
        let state = state(theme);
        insta::assert_snapshot!(format!("vault_theme_{theme}"), render(&state, 140, 40));
    }
}

#[test]
fn unlock_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Unlock;
    state.session = SessionPhase::Locked;
    state.unlock.password = "secret".into();
    insta::assert_snapshot!("unlock_120x36", render(&state, 120, 36));
}

#[test]
fn login_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Login;
    state.session = SessionPhase::LoggedOut;
    insta::assert_snapshot!("login_120x36", render(&state, 120, 36));
}

#[test]
fn generator_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Generator;
    state.tab = Tab::Generator;
    state.generator.output = "Rv8%qTn!Ze3wLx&p6MuJ#c2yB".into();
    state.generator.request.length = 24;
    state.generator.history = vec![
        ("Rv8%qTn!Ze3wLx&p6MuJ#c2yB".into(), std::time::Instant::now()),
        ("anchor-marble-crisp-velvet-7".into(), std::time::Instant::now()),
    ];
    insta::assert_snapshot!("generator_120x36", render(&state, 120, 36));
}

#[test]
fn health_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Health;
    state.tab = Tab::Health;
    let items: Vec<Item> = state.cache.active_items().cloned().collect();
    let report = lazybw::vault::health::build(&items, &[], state.now, 365, true);
    state.health.report = Some(report);
    state.cache.apply_health(state.health.report.as_ref().unwrap());
    state.health_epoch += 1;
    insta::assert_snapshot!("health_120x36", render(&state, 120, 36));
}

#[test]
fn editor_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Editor;
    let item = state.cache.item("id-github").unwrap().clone();
    state.editor = Some(EditorState {
        item: item.clone(),
        is_new: false,
        original_item: item,
        rows: vec![
            FormRow {
                key: FormKey::Name,
                label: "Name".into(),
                value: "GitHub".into(),
                kind: FormKind::Text,
                section: Some("CREDENTIALS"),
                hint: None,
            },
            FormRow {
                key: FormKey::Folder,
                label: "Folder".into(),
                value: "Personal".into(),
                kind: FormKind::Option(vec!["Personal".into()]),
                section: None,
                hint: Some("Enter pick".into()),
            },
            FormRow {
                key: FormKey::Username,
                label: "Username".into(),
                value: "dev-example".into(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            },
            FormRow {
                key: FormKey::Password,
                label: "Password".into(),
                value: "k7#Qm!vR2x@Lp9$Wn4eT".into(),
                kind: FormKind::Secret,
                section: None,
                hint: Some("Ctrl-g generate".into()),
            },
            FormRow {
                key: FormKey::Favorite,
                label: "Favorite".into(),
                value: "true".into(),
                kind: FormKind::Bool,
                section: Some("OPTIONS"),
                hint: None,
            },
            FormRow {
                key: FormKey::Notes,
                label: "Notes".into(),
                value: "Recovery codes are in the safe, drawer 2.".into(),
                kind: FormKind::Notes,
                section: Some("NOTES"),
                hint: Some("Ctrl-e $EDITOR".into()),
            },
        ],
        row: 2,
        offset: 0,
        insert: true,
        cursor: 11,
        revealed: false,
        dirty: true,
        errors: Default::default(),
        notes_offset: 0,
        remote_change: false,
        history: Default::default(),
    });
    insta::assert_snapshot!("editor_120x36", render(&state, 120, 36));
}

#[test]
fn editor_normal_mode_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Editor;
    let item = state.cache.item("id-github").unwrap().clone();
    state.editor = Some(EditorState {
        item: item.clone(),
        is_new: false,
        original_item: item,
        rows: vec![
            FormRow {
                key: FormKey::Name,
                label: "Name".into(),
                value: "GitHub".into(),
                kind: FormKind::Text,
                section: Some("CREDENTIALS"),
                hint: None,
            },
            FormRow {
                key: FormKey::Username,
                label: "Username".into(),
                value: "dev-example".into(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            },
            FormRow {
                key: FormKey::Password,
                label: "Password".into(),
                value: "k7#Qm!vR2x@Lp9$Wn4eT".into(),
                kind: FormKind::Secret,
                section: None,
                hint: Some("Ctrl-g generate".into()),
            },
            FormRow {
                key: FormKey::Notes,
                label: "Notes".into(),
                value: "Recovery codes are in the safe, drawer 2.".into(),
                kind: FormKind::Notes,
                section: Some("NOTES"),
                hint: Some("Ctrl-e $EDITOR".into()),
            },
        ],
        row: 2,
        offset: 0,
        insert: false,
        cursor: 20,
        revealed: false,
        dirty: false,
        errors: Default::default(),
        notes_offset: 0,
        remote_change: false,
        history: Default::default(),
    });
    insta::assert_snapshot!("editor_normal_120x36", render(&state, 120, 36));
}

#[test]
fn vault_every_item_type_at_every_size() {
    let kinds = [
        ("login", "id-github"),
        ("card", "id-card"),
        ("identity", "id-identity"),
        ("note", "id-note"),
        ("ssh", "id-ssh"),
        ("other", "id-other"),
    ];
    for (label, id) in kinds {
        for (width, height) in [(160u16, 45u16), (120, 36), (90, 30), (80, 24)] {
            let mut state = state("tokyonight-night");
            let rows = state.visible_rows();
            state.list_row = rows.iter().position(|row| row.id == id).expect("row");
            state.focus = lazybw::app::state::Focus::Detail;
            insta::assert_snapshot!(
                format!("vault_{label}_{width}x{height}"),
                render(&state, width, height)
            );
        }
    }
}

#[test]
fn settings_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Settings;
    insta::assert_snapshot!("settings_120x36", render(&state, 120, 36));
}

#[test]
fn keymap_editor_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::KeymapEditor;
    state.keymap_editor.contexts = vec!["All".into(), "list".into()];
    state.keymap_editor.context_index = 1;
    insta::assert_snapshot!("keymap_editor_120x36", render(&state, 120, 36));
}

#[test]
fn help_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Help;
    insta::assert_snapshot!("help_120x36", render(&state, 120, 36));
}

#[test]
fn messages_screen() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::Messages;
    state.messages.push(lazybw::app::state::LogMessage {
        time: state.now,
        level: MessageLevel::Ok,
        text: "Synced 247 items.".into(),
    });
    state.messages.push(lazybw::app::state::LogMessage {
        time: state.now,
        level: MessageLevel::Error,
        text: "Couldn't save “GitHub”: Vault is locked.".into(),
    });
    insta::assert_snapshot!("messages_120x36", render(&state, 120, 36));
}

#[test]
fn quick_open_overlay() {
    let mut state = state("tokyonight-night");
    state.open(QuickOpenState {
        input: "git".into(),
        cursor: 3,
        results: vec!["id-github".into(), "id-gmail".into()],
        index: 0,
        offset: 0,
        ignored_health: false,
    });
    insta::assert_snapshot!("quick_open_140x40", render(&state, 140, 40));
}

#[test]
fn picker_overlay() {
    let mut state = state("tokyonight-night");
    state.open(PickerState::new(
        "New item",
        PickerKind::ItemType,
        vec![
            PickerEntry {
                label: "login".into(),
                note: String::new(),
                filter: "login".into(),
                value: "login".into(),
                accelerator: Some('l'),
            },
            PickerEntry {
                label: "secure note".into(),
                note: String::new(),
                filter: "note".into(),
                value: "note".into(),
                accelerator: Some('n'),
            },
            PickerEntry {
                label: "card".into(),
                note: String::new(),
                filter: "card".into(),
                value: "card".into(),
                accelerator: Some('c'),
            },
        ],
    ));
    insta::assert_snapshot!("picker_120x36", render(&state, 120, 36));
}

#[test]
fn confirm_overlay() {
    let mut state = state("tokyonight-night");
    state.open(
        ConfirmState::new(
            "Delete permanently?",
            "“Old forum” will be deleted for good. This cannot be undone.",
            "Delete",
            lazybw::app::state::Deferred::DeleteItems(Vec::new()),
        )
        .danger(),
    );
    insta::assert_snapshot!("confirm_120x36", render(&state, 120, 36));
}

#[test]
fn too_small_screen() {
    let state = state("tokyonight-night");
    insta::assert_snapshot!("too_small_52x14", render(&state, 52, 14));
}

#[test]
fn too_small_screen_narrow() {
    let state = state("tokyonight-night");
    insta::assert_snapshot!("too_small_40x10", render(&state, 40, 10));
}

#[test]
fn export_wizard_snapshot() {
    let mut state = state("tokyonight-night");
    state.open(ExportState {
        format: lazybw::bw::service::ExportFormat::Json,
        path: "~/bitwarden_export_20261030140000.json".into(),
        ..Default::default()
    });
    insta::assert_snapshot!("export_wizard_120x36", render(&state, 120, 36));
}

#[test]
fn import_preview_snapshot() {
    let mut state = state("tokyonight-night");
    let named = |name: &str, kind: ItemType| {
        let mut item = Item::new(kind);
        item.name = name.into();
        item
    };
    let duplicates = vec![
        lazybw::vault::dupes::Duplicate {
            index: 0,
            name: "GitHub".into(),
            kind: ItemType::Login,
            detail: "dev-example".into(),
        },
        lazybw::vault::dupes::Duplicate {
            index: 1,
            name: "Gmail".into(),
            kind: ItemType::Login,
            detail: "personal@example.com".into(),
        },
        lazybw::vault::dupes::Duplicate {
            index: 2,
            name: "Router notes".into(),
            kind: ItemType::SecureNote,
            detail: "admin / hunter2".into(),
        },
    ];
    state.open(ImportState {
        step: ImportStep::Preview(ImportPreview {
            items: vec![
                named("GitHub", ItemType::Login),
                named("Gmail", ItemType::Login),
                named("Router notes", ItemType::SecureNote),
                named("Visa", ItemType::Card),
                named("Passport", ItemType::Identity),
            ],
            skip: duplicates.iter().map(|d| d.index).collect(),
            skip_enabled: true,
            duplicates,
            ..Default::default()
        }),
        format: "bitwardenjson".into(),
        path: "~/Downloads/bitwarden_export_20260101.json".into(),
        ..Default::default()
    });
    insta::assert_snapshot!("import_preview_120x36", render(&state, 120, 36));
}

#[test]
fn history_popup_snapshot() {
    let mut state = state("tokyonight-night");
    state.open(HistoryState {
        item_id: "id-github".into(),
        entries: vec![
            lazybw::bw::models::PasswordHistory {
                last_used_date: Some("2026-03-01T10:00:00Z".into()),
                password: lazybw::bw::sensitive::Sensitive::new("old-password-1"),
                ..Default::default()
            },
            lazybw::bw::models::PasswordHistory {
                last_used_date: Some("2025-01-01T10:00:00Z".into()),
                password: lazybw::bw::sensitive::Sensitive::new("old-password-2"),
                ..Default::default()
            },
        ],
        index: 0,
        revealed: Default::default(),
        current: Some("current-password".into()),
    });
    insta::assert_snapshot!("history_popup_120x36", render(&state, 120, 36));
}

#[test]
fn info_popup_snapshot() {
    let mut state = state("tokyonight-night");
    state.open(InfoState {
        item_id: "id-github".into(),
        lines: vec![
            ("Name".into(), "GitHub".into()),
            ("Type".into(), "login".into()),
            ("Folder".into(), "Personal".into()),
            ("Created".into(), "2023-03-12T09:30:00Z".into()),
            ("Edited".into(), "2026-10-27T09:30:00Z".into()),
            ("Master password".into(), "not required".into()),
            ("Id".into(), "id-github".into()),
        ],
    });
    insta::assert_snapshot!("info_popup_120x36", render(&state, 120, 36));
}

#[test]
fn reprompt_popup_snapshot() {
    let mut state = state("tokyonight-night");
    state.open(RepromptState {
        item_id: "id-github".into(),
        item_name: "Bank".into(),
        password: "secret".into(),
        error: None,
        ..Default::default()
    });
    insta::assert_snapshot!("reprompt_popup_120x36", render(&state, 120, 36));
}

#[test]
fn which_key_snapshot() {
    let mut state = state("tokyonight-night");
    state.which_key = Some(WhichKeyState {
        prefix: "Space".into(),
        continuations: vec![
            ("n".into(), false, "New item".into()),
            ("f".into(), true, "+folders".into()),
            ("g".into(), false, "Open the generator".into()),
            ("h".into(), false, "Open vault health".into()),
            ("/".into(), false, "Find any item".into()),
            ("s".into(), true, "+sort".into()),
            ("t".into(), false, "Choose a theme".into()),
            ("e".into(), false, "Export the vault".into()),
        ],
        since: std::time::Instant::now(),
    });
    insta::assert_snapshot!("which_key_120x36", render(&state, 120, 36));
}

#[test]
fn wildmenu_snapshot() {
    let mut state = state("tokyonight-night");
    state.open(CmdlineState {
        input: "theme tok".into(),
        cursor: 9,
        completions: vec![
            "tokyonight-night".into(),
            "tokyonight-storm".into(),
            "tokyonight-moon".into(),
            "tokyonight-day".into(),
        ],
        completion_index: 0,
        show_completions: true,
        ..Default::default()
    });
    insta::assert_snapshot!("wildmenu_120x36", render(&state, 120, 36));
}

#[test]
fn keymap_capture_with_conflict_snapshot() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::KeymapEditor;
    state.keymap_editor.contexts = vec!["All".into(), "list".into()];
    state.keymap_editor.context_index = 1;
    state.keymap_editor.capture = Some(CaptureState {
        action: "item.favorite".into(),
        context: "list".into(),
        keys: vec!["g".into(), "f".into()],
        conflict: Some("gf is used by “Go to this item's folder”.".into()),
        adding: false,
    });
    insta::assert_snapshot!("keymap_capture_conflict_120x36", render(&state, 120, 36));
}

#[test]
fn picker_window_follows_the_cursor() {
    let mut state = state("tokyonight-night");
    let entries = (0..30)
        .map(|i| PickerEntry {
            label: format!("Folder {i:02}"),
            note: String::new(),
            filter: format!("folder {i:02}"),
            value: format!("Folder {i:02}"),
            accelerator: None,
        })
        .collect();
    state.open(PickerState::new("Go to folder", PickerKind::FolderJump, entries));
    let selected_line = |state: &AppState| {
        render(state, 120, 36)
            .lines()
            .find(|line| line.contains("▌Folder") || line.contains("▌  Folder"))
            .map(str::to_owned)
    };
    for index in [0, 11, 12, 20, 29, 0] {
        let Some(Overlay::Picker(picker)) = &mut state.overlay else { panic!("picker") };
        picker.index = index;
        let line =
            selected_line(&state).unwrap_or_else(|| panic!("cursor at {index} is not drawn"));
        assert!(line.contains(&format!("Folder {index:02}")), "{index}: {line}");
    }
}

#[test]
fn toasts_wrap_instead_of_cutting_the_message() {
    let mut state = state("tokyonight-night");
    let title = "Copied username for “A rather long item name here”. Clipboard clears in 30s.";
    state.toast(lazybw::app::state::MessageLevel::Ok, title);
    let screen = render(&state, 120, 36);
    for part in
        ["✓ Copied username for “A rather long item name here”.", "Clipboard clears in 30s."]
    {
        assert!(screen.contains(part), "{part:?} missing from the toast:\n{screen}");
    }
}

#[test]
fn keymap_editor_scrolls_with_the_cursor() {
    let mut state = state("tokyonight-night");
    state.screen = Screen::KeymapEditor;
    state.keymap_editor.contexts = vec!["All".into(), "list".into()];
    state.keymap_editor.context_index = 1;
    let rows = lazybw::app::update::keymap_rows(&state);
    assert!(rows.len() > 40, "needs more rows than fit: {}", rows.len());
    for index in [0, 20, 35, rows.len() - 1, 3] {
        state.keymap_editor.index = index;
        let screen = render(&state, 120, 36);
        let line = screen
            .lines()
            .find(|line| line.starts_with("│▌"))
            .unwrap_or_else(|| panic!("cursor at {index} is not drawn"));
        assert!(line.contains(&rows[index].1), "{index}: {line}");
        assert!(screen.contains("DESCRIPTION"), "header scrolled away");
    }
}

#[test]
fn folder_list_title_shows_the_folder_path() {
    let mut state = state("tokyonight-night");
    state.filter = lazybw::vault::cache::Filter::Folder("Work/Projects".to_owned());
    let screen = render(&state, 140, 40);
    assert!(screen.contains("Work › Projects 0"), "{screen}");
    assert!(!screen.contains(" Folder "), "{screen}");
    // Narrow list pane: the path keeps its end.
    state.filter = lazybw::vault::cache::Filter::Folder(
        "A very long parent folder name/And another level/Projects".to_owned(),
    );
    let screen = render(&state, 120, 36);
    assert!(screen.contains("… ") || screen.contains("…"), "{screen}");
    assert!(screen.contains("Projects 0"), "{screen}");
    if std::env::var_os("LAZYBW_PRINT_SCREEN").is_some() {
        println!("{}", screen.lines().nth(1).unwrap_or_default());
    }
}

/// The bottom rows (statusline, message line) as (symbol, fg, bg) cells.
fn bottom_rows(state: &AppState, width: u16, height: u16) -> Vec<(String, String, String)> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| lazybw::ui::draw(frame, state)).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut out = Vec::new();
    for y in height - 2..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            out.push((
                cell.symbol().to_owned(),
                format!("{:?}", cell.fg),
                format!("{:?}", cell.bg),
            ));
        }
    }
    out
}

#[test]
fn docked_popups_leave_the_statusline_and_command_line_alone() {
    let mut state = state("tokyonight-night");
    let plain = bottom_rows(&state, 120, 36);
    state.which_key = Some(WhichKeyState {
        prefix: "Space".into(),
        continuations: (0..20)
            .map(|i| (format!("{}", (b'a' + i) as char), false, format!("Action {i}")))
            .collect(),
        since: std::time::Instant::now(),
    });
    assert_eq!(bottom_rows(&state, 120, 36), plain, "which-key changed the bottom rows");
    state.which_key = None;
    let mut cmdline = CmdlineState {
        input: "theme tok".into(),
        cursor: 9,
        completions: vec!["tokyonight-night".into(), "tokyonight-storm".into()],
        completion_index: 0,
        show_completions: false,
        ..Default::default()
    };
    state.open(cmdline.clone());
    let typing = bottom_rows(&state, 120, 36);
    cmdline.show_completions = true;
    state.open(cmdline);
    assert_eq!(bottom_rows(&state, 120, 36), typing, "the wildmenu changed the bottom rows");
}

#[test]
fn sidebar_marks_parent_folders_with_a_fold_arrow() {
    let mut state = state("tokyonight-night");
    state.config.ui.icons = lazybw::config::IconMode::Nerdfont;
    let screen = render(&state, 140, 40);
    let work = screen.lines().find(|l| l.contains("Personal")).unwrap().to_owned();
    let projects = screen.lines().find(|l| l.contains("Banking")).unwrap().to_owned();
    assert!(work.contains('▾'), "{work}");
    assert!(!projects.contains('▾') && !projects.contains('▸'), "{projects}");
    state.collapsed.insert("Personal".to_owned());
    let screen = render(&state, 140, 40);
    let work = screen.lines().find(|l| l.contains("Personal")).unwrap().to_owned();
    assert!(work.contains('▸'), "{work}");
    // Unicode: leaves get a dot, not a closed-folder arrow.
    state.config.ui.icons = lazybw::config::IconMode::Unicode;
    state.collapsed.clear();
    let screen = render(&state, 140, 40);
    let projects = screen.lines().find(|l| l.contains("Banking")).unwrap().to_owned();
    assert!(projects.contains('·') && !projects.contains('▸'), "{projects}");
}

/// The text of a render, without the style section.
fn text_of(state: &AppState, width: u16, height: u16) -> String {
    let screen = render(state, width, height);
    screen.split("styles:").next().unwrap_or_default().to_owned()
}

#[test]
fn narrow_terminals_keep_the_essentials() {
    // 80 columns: the session state wins over the email and server.
    let mut state = state("tokyonight-night");
    state.email = Some("someone.with.a.long.address@example.com".to_owned());
    state.server = Some("https://vault.bitwarden.eu".to_owned());
    let screen = text_of(&state, 80, 24);
    let topbar = screen.lines().next().unwrap();
    assert!(topbar.contains("unlocked"), "{topbar}");

    // Quick open at 80 columns: the way out is always listed, and each row
    // carries its subtitle when there is no preview pane.
    state.open(QuickOpenState {
        input: "g".into(),
        cursor: 1,
        results: vec!["id-github".into(), "id-gmail".into()],
        index: 0,
        offset: 0,
        ignored_health: false,
    });
    let screen = text_of(&state, 80, 24);
    assert!(screen.contains("Esc close"), "{screen}");
    assert!(screen.contains("dev-example"), "{screen}");
    state.take_quick_open();

    // Health at 60 columns: all five categories stay visible.
    state.screen = Screen::Health;
    state.tab = Tab::Health;
    let items: Vec<Item> = state.cache.active_items().cloned().collect();
    let report = lazybw::vault::health::build(&items, &[], state.now, 365, true);
    state.health.report = Some(report);
    let screen = text_of(&state, 60, 20);
    for label in ["exposed", "reused", "weak", "over 1 year", "no HTTPS"] {
        assert!(screen.contains(label), "{label} missing at 60 columns:\n{screen}");
    }
}
