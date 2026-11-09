//! End-to-end flows driven by key sequences against `fake-bw`.

mod harness;

use harness::{assert_message, ready_app};
use lazybw::app::Screen;
use lazybw::app::state::Overlay;
use lazybw::bw::models::ItemType;
use lazybw::vault::cache::Filter;

#[tokio::test]
async fn startup_loads_the_vault() {
    let mut app = ready_app().await;
    assert_eq!(app.app.state.screen, Screen::Vault);
    assert_eq!(app.app.state.cache.len(), 5);
    assert_eq!(app.app.state.email.as_deref(), Some("you@example.com"));
    assert_eq!(app.app.state.cache.count(&Filter::Favorites), 1);
    app.wait_idle().await;
    // The first list row is GitHub, alphabetically before Gmail.
    let rows = app.app.state.visible_rows();
    assert_eq!(rows[0].name, "GitHub");
}

#[tokio::test]
async fn movement_and_detail() {
    let mut app = ready_app().await;
    app.keys("j").await;
    assert_eq!(app.app.state.list_row, 1);
    app.keys("<CR>").await;
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::Detail);
    assert_eq!(app.app.state.selected_item().unwrap().name, "Gmail");
    app.keys("gg").await;
    assert_eq!(app.app.state.list_row, 1, "detail movement moves fields, not items");
    app.keys("h").await;
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::List);
}

#[tokio::test]
async fn edit_renames_an_item() {
    let mut app = ready_app().await;
    app.keys("e").await;
    assert_eq!(app.app.state.screen, Screen::Editor);
    assert!(app.app.state.editor.is_some());
    // The Name row is selected; append "X" and save/close.
    app.keys("iX<Esc>:wq<CR>").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.screen, Screen::Vault);
    assert!(app.fake_names().iter().any(|name| name == "XGitHub"), "{:?}", app.fake_names());
    assert!(
        app.app.state.cache.active_items().any(|item| item.name == "XGitHub"),
        "cache: {:?}",
        app.app.state.cache.active_items().map(|i| i.name.clone()).collect::<Vec<_>>()
    );
    let put = app.requests().iter().any(|entry| entry["request"]["method"] == "PUT");
    assert!(put, "expected a PUT request, got {:?}", app.requests());
}

#[tokio::test]
async fn trash_and_undo() {
    let mut app = ready_app().await;
    app.keys("dd").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.cache.count(&Filter::All), 4);
    assert_eq!(app.app.state.cache.count(&Filter::Trash), 1);
    assert_message(&app, "Moved");
    app.keys("u").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.cache.count(&Filter::All), 5);
    assert_eq!(app.app.state.cache.count(&Filter::Trash), 0);
    assert_message(&app, "Undone");
}

#[tokio::test]
async fn search_filters_and_clears() {
    let mut app = ready_app().await;
    app.keys("/gma").await;
    assert_eq!(app.app.state.visible_rows().len(), 1);
    assert_eq!(app.app.state.visible_rows()[0].name, "Gmail");
    app.keys("<CR>").await;
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::List);
    app.keys("/").await;
    app.keys("<Esc>").await;
    assert_eq!(app.app.state.visible_rows().len(), 5);
}

#[tokio::test]
async fn quick_open_and_copy() {
    let mut app = ready_app().await;
    app.keys("<C-p>git<CR>").await;
    assert_eq!(app.app.state.screen, Screen::Vault);
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::Detail);
    assert_eq!(app.app.state.selected_item().unwrap().name, "GitHub");
    app.keys("<C-p>git<C-y>").await;
    // The clipboard is the memory backend, so we can inspect it.
    assert_eq!(app.app.state.clipboard_label.as_deref(), Some("password"));
}

#[tokio::test]
async fn create_a_new_login() {
    let mut app = ready_app().await;
    app.keys("a").await;
    assert!(app.app.state.picker().is_some());
    app.keys("l").await;
    assert_eq!(app.app.state.screen, Screen::Editor);
    app.keys("iNew account<Esc>").await;
    // Move to the username row and fill it.
    app.keys("jji").await;
    app.keys("new@example.com<Esc>").await;
    app.keys(":wq<CR>").await;
    app.wait_idle().await;
    assert!(app.fake_names().iter().any(|name| name == "New account"), "{:?}", app.fake_names());
    assert_eq!(app.app.state.cache.active_items().filter(|i| i.kind == ItemType::Login).count(), 3);
}

#[tokio::test]
async fn lock_and_unlock() {
    let mut app = ready_app().await;
    app.keys("<Space>l").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.screen, Screen::Unlock);
    assert_eq!(app.app.state.cache.len(), 0);
    app.keys("wrong<CR>").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.unlock.error.as_deref(), Some("Invalid master password."));
    app.keys("correct horse<CR>").await;
    app.wait_ready().await;
    assert_eq!(app.app.state.cache.len(), 5);
}

#[tokio::test]
async fn folder_filter_and_counts() {
    let mut app = ready_app().await;
    app.keys("<Space>1").await;
    let entries =
        lazybw::vault::filter::entries(&app.app.state.cache, &app.app.state.collapsed, false);
    let work = entries.iter().position(|e| e.label == "Work").expect("Work folder");
    app.app.state.sidebar_row = work;
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    app.keys("<CR>").await;
    assert_eq!(app.app.state.filter, Filter::Folder("Work".to_owned()));
    assert_eq!(app.app.state.visible_rows().len(), 2);
}

#[tokio::test]
async fn generator_produces_a_password() {
    let mut app = ready_app().await;
    app.keys("<Space>g").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.screen, Screen::Generator);
    assert!(!app.app.state.generator.output.is_empty());
    assert!(app.app.state.generator.output.chars().count() >= 5);
}

#[tokio::test]
async fn health_report_runs() {
    let mut app = ready_app().await;
    // The fake-bw capabilities say password_health is available.
    app.keys("<Space>h").await;
    app.wait_idle().await;
    assert!(
        app.app.state.health.report.is_some(),
        "messages={:?} pending={:?} running={}",
        app.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>(),
        app.app.state.pending,
        app.app.state.health.running
    );
    let report = app.app.state.health.report.as_ref().unwrap();
    assert_eq!(report.checked, 2);
}

#[tokio::test]
async fn cmdline_theme_and_sync() {
    let mut app = ready_app().await;
    app.keys(":theme nord<CR>").await;
    assert_eq!(app.app.state.theme.name, "nord");
    app.keys(":sync<CR>").await;
    app.wait_idle().await;
    assert_message(&app, "Synced");
    app.keys(":nonsense<CR>").await;
    assert_message(&app, "E492");
}

#[tokio::test]
async fn which_key_appears_after_the_prefix() {
    let mut app = ready_app().await;
    app.keys("<Space>").await;
    // The prefix is pending immediately; which-key appears after 300 ms.
    assert!(app.app.state.dispatcher.is_pending());
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    app.handle(lazybw::app::Msg::Tick).await;
    assert!(app.app.state.which_key.is_some(), "which-key opens after which_key_delay");
    // Long after timeoutlen the leader alone is still pending: a pure prefix
    // waits for the next key instead of closing which-key under the reader.
    app.app.state.dispatcher.since =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(10));
    app.handle(lazybw::app::Msg::Tick).await;
    assert!(app.app.state.dispatcher.is_pending());
    assert!(app.app.state.which_key.is_some(), "which-key must stay open");
    let labels: Vec<String> = app
        .app
        .state
        .which_key
        .as_ref()
        .unwrap()
        .continuations
        .iter()
        .map(|(_, _, label)| label.clone())
        .collect();
    assert!(labels.iter().any(|l| l == "+folders"), "{labels:?}");
    assert!(labels.iter().any(|l| l == "+sort"), "{labels:?}");
    app.keys("<Esc>").await;
    assert!(!app.app.state.dispatcher.is_pending());
    assert!(app.app.state.which_key.is_none());
}

#[tokio::test]
async fn export_json_writes_a_private_file() {
    let mut app = ready_app().await;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("export.json");
    app.keys(&format!(":export json {}<CR>", path.display())).await;
    assert!(app.app.state.export().is_some());
    // Step 1: format and destination.
    app.keys("<CR>").await;
    // Step 2: master password.
    app.keys("correct horse<CR>").await;
    app.wait_idle().await;
    assert!(path.exists(), "export file missing");
    let metadata = std::fs::metadata(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn import_previews_duplicates_and_skips_them() {
    let mut app = ready_app().await;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("import.json");
    let fixture = harness::fixture_vault();
    let mut items = fixture["items"].as_array().unwrap().clone();
    // A duplicate of GitHub plus one new login.
    let mut new_item = serde_json::json!({
        "object": "item", "id": "import-1", "type": 1, "name": "New service",
        "login": {"username": "import@example.com", "password": "import-password", "uris": []}
    });
    items.push(new_item.clone());
    new_item["id"] = serde_json::json!("import-2");
    new_item["login"]["username"] = serde_json::json!("another@example.com");
    new_item["login"]["uri"] = serde_json::json!(null);
    items.push(new_item);
    let body = serde_json::json!({"encrypted": false, "items": items, "folders": []});
    std::fs::write(&path, serde_json::to_vec(&body).unwrap()).unwrap();
    app.keys(&format!(":import bitwardenjson {}<CR>", path.display())).await;
    assert!(app.app.state.import().is_some());
    assert_eq!(app.app.state.import().unwrap().step, 1);
    app.keys("<CR>").await; // read the file -> preview
    let import = app.app.state.import().unwrap();
    assert_eq!(import.step, 2);
    assert_eq!(import.duplicates.len(), 5);
    assert_eq!(import.count, 7);
    app.keys("<CR>").await; // import
    app.wait_idle().await;
    assert!(
        app.fake_names().iter().any(|name| name == "New service"),
        "names={:?} messages={:?}",
        app.fake_names(),
        app.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>()
    );
    // The duplicate GitHub was skipped, the new items imported.
    let githubs = app.fake_names().iter().filter(|name| *name == "GitHub").count();
    assert_eq!(githubs, 1);
    assert_eq!(app.app.state.cache.active_items().filter(|i| i.kind == ItemType::Login).count(), 4);
}

#[tokio::test]
async fn mouse_click_selects_and_switches_tabs() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut app = ready_app().await;
    app.app.state.size = (140, 40);
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::List);
    assert_eq!(app.app.state.list_row, 4);
    // Click the Generator tab in the topbar.
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 24,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert_eq!(app.app.state.tab, lazybw::app::state::Tab::Generator);
    // Wheel scrolling moves the list.
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 40,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert!(app.app.state.list_row <= 4);
}

#[tokio::test]
async fn mouse_drag_resizes_the_sidebar() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut app = ready_app().await;
    app.app.state.size = (160, 40);
    // The sidebar's right border is at x = 25.
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 25,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert!(app.app.state.drag.is_some());
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 33,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert_eq!(app.app.state.sidebar_width, Some(33));
    app.handle(lazybw::app::Msg::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 33,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .await;
    assert!(app.app.state.drag.is_none());
}

#[tokio::test]
async fn notes_open_in_edtui_and_save() {
    let mut app = ready_app().await;
    app.keys("e").await;
    let notes_row = app
        .app
        .state
        .editor
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|row| row.key == lazybw::app::state::FormKey::Notes)
        .unwrap();
    app.app.state.editor.as_mut().unwrap().row = notes_row;
    app.keys("<CR>").await; // opens the notes editor
    assert!(app.app.state.notes.is_some());
    app.keys("ihello from edtui<Esc>").await;
    app.keys("<Tab>").await; // leave the notes field
    assert!(app.app.state.notes.is_none());
    let row = app
        .app
        .state
        .editor
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|row| row.key == lazybw::app::state::FormKey::Notes)
        .unwrap();
    assert!(row.value.contains("hello from edtui"), "{}", row.value);
    app.keys(":wq<CR>").await;
    app.wait_idle().await;
    let state = app.fake_state();
    let github =
        state["items"].as_array().unwrap().iter().find(|item| item["id"] == "item-1").unwrap();
    assert!(github["notes"].as_str().unwrap().contains("hello from edtui"));
}

#[tokio::test]
async fn passkey_can_be_removed() {
    let mut app = ready_app().await;
    app.app.state.focus = lazybw::app::state::Focus::Detail;
    let index = lazybw::app::update::detail_fields(&app.app.state)
        .iter()
        .position(|(key, _, _)| key == "passkey")
        .expect("passkey field");
    app.app.state.detail.row = index;
    app.keys("dd").await;
    assert!(app.app.state.confirm().is_some());
    app.keys("y").await;
    app.wait_idle().await;
    let state = app.fake_state();
    let github =
        state["items"].as_array().unwrap().iter().find(|item| item["id"] == "item-1").unwrap();
    assert_eq!(github["login"]["fido2Credentials"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn folder_rename_and_delete_include_subfolders() {
    let mut app = ready_app().await;
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    let entries = lazybw::vault::filter::entries(&app.app.state.cache, &Default::default(), false);
    let work = entries.iter().position(|entry| entry.label == "Work").unwrap();
    app.app.state.sidebar_row = work;
    app.keys("r").await;
    assert!(app.app.state.prompt().is_some());
    // The prompt is prefilled with the current name; clear it first.
    app.keys("<BS><BS><BS><BS>Job<CR>").await;
    assert!(app.app.state.confirm().is_some(), "rename confirmation");
    // Also-rename is on by default.
    app.keys("<CR>").await;
    app.wait_idle().await;
    let names: Vec<String> = app.fake_state()["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
        .collect();
    assert!(names.contains(&"Job".to_owned()), "{names:?}");
    assert!(names.contains(&"Job/Projects".to_owned()), "{names:?}");
    // Delete both: the checkbox starts off and is toggled with space.
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    let entries = lazybw::vault::filter::entries(&app.app.state.cache, &Default::default(), false);
    let job = entries.iter().position(|entry| entry.label == "Job").unwrap();
    app.app.state.sidebar_row = job;
    app.keys("dd").await;
    assert!(app.app.state.confirm().is_some());
    assert_eq!(app.app.state.confirm().unwrap().subfolder_count, 1);
    app.keys(" ").await;
    assert!(app.app.state.confirm().unwrap().also_subfolders);
    app.keys("<CR>").await;
    app.wait_idle().await;
    let names: Vec<String> = app.fake_state()["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
        .collect();
    println!(
        "MESSAGES {:?} NAMES {:?}",
        app.app
            .state
            .messages
            .iter()
            .map(|m| (m.text.clone(), format!("{:?}", m.level)))
            .collect::<Vec<_>>(),
        names
    );
    assert!(!names.iter().any(|name| name.starts_with("Job")), "{names:?}");
}

#[tokio::test]
async fn motion_is_gated_by_the_animation_setting() {
    let mut app = ready_app().await;
    // The test harness disables animations by default.
    app.app.state.fx.borrow_mut().set_enabled(false);
    app.keys("yp").await;
    assert!(!app.app.state.fx.borrow().flash_active());
    app.app.state.fx.borrow_mut().set_enabled(true);
    app.keys("yp").await;
    assert!(app.app.state.fx.borrow().flash_active());
    app.keys("<Space>2").await;
    assert!(app.app.state.fx.borrow().is_active(), "tab fade/sweep should run");
}

#[tokio::test]
async fn editor_json_round_trip_with_scripted_editor() {
    let editor = harness::workspace_file("scripts/fake-editor");
    let mut app = ready_app().await.with_editor(editor);
    app.keys("e").await; // open the editor for GitHub
    app.keys("gE").await; // whole item as JSON in $EDITOR
    assert!(app.app.state.editor.is_some());
    assert!(
        app.app.state.editor.as_ref().unwrap().item.name.contains("edited by fake-editor"),
        "name after $EDITOR: {}",
        app.app.state.editor.as_ref().unwrap().item.name
    );
    app.keys(":wq<CR>").await;
    app.wait_idle().await;
    assert!(
        app.fake_names().iter().any(|name| name.contains("edited by fake-editor")),
        "{:?}",
        app.fake_names()
    );
}

#[tokio::test]
async fn password_protected_export_goes_through_the_pty() {
    let mut app = ready_app().await;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("export.enc.json");
    app.keys(&format!(":export encrypted {}<CR>", path.display())).await;
    assert!(app.app.state.export().is_some());
    app.keys("<CR>").await; // format -> password step
    app.keys("filepass").await;
    app.keys("<Tab>").await; // confirmation
    app.keys("filepass").await;
    app.keys("<CR>").await; // master password step
    app.keys("correct horse<CR>").await;
    app.wait_idle().await;
    assert!(path.exists(), "protected export missing");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(parsed["encrypted"], true);
    assert_eq!(parsed["passwordProtected"], true);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("fixture-password"), "plaintext leaked into the protected export");
    // The request went through the PTY, not argv: no password in any command.
    for entry in app.requests() {
        let argv = serde_json::to_string(&entry["cmd"]).unwrap();
        assert!(!argv.contains("filepass"), "file password in argv: {argv}");
    }
}

#[tokio::test]
async fn password_protected_import_uses_the_file_password() {
    let mut app = ready_app().await;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("protected.json");
    let body = serde_json::json!({
        "encrypted": true,
        "passwordProtected": true,
        "password": "filepass",
        "data": {
            "encrypted": false,
            "items": [{
                "object": "item", "id": "protected-1", "type": 1, "name": "Protected item",
                "login": {"username": "protected@example.com", "password": "protected-password"}
            }],
            "folders": []
        }
    });
    std::fs::write(&path, serde_json::to_vec(&body).unwrap()).unwrap();
    app.keys(&format!(":import bitwardenpasswordprotected {}<CR>", path.display())).await;
    // With a format on the command line the wizard opens at the file step.
    assert_eq!(app.app.state.import().unwrap().step, 1);
    app.keys("<Tab>").await; // password field
    app.keys("filepass").await;
    app.keys("<CR>").await; // run the import through the subprocess
    app.wait_idle().await;
    assert!(
        app.fake_names().iter().any(|name| name == "Protected item"),
        "names={:?} messages={:?}",
        app.fake_names(),
        app.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>()
    );
    for entry in app.requests() {
        let argv = serde_json::to_string(&entry["cmd"]).unwrap();
        assert!(!argv.contains("filepass"), "file password in argv: {argv}");
    }
}

#[tokio::test]
async fn folder_rename_undo_and_redo_restores_names() {
    let mut app = ready_app().await;
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    let entries = lazybw::vault::filter::entries(&app.app.state.cache, &Default::default(), false);
    app.app.state.sidebar_row = entries.iter().position(|entry| entry.label == "Work").unwrap();
    app.keys("r<BS><BS><BS><BS>Job<CR><CR>").await;
    app.wait_idle().await;
    let names = |app: &harness::TestApp| -> Vec<String> {
        app.fake_state()["folders"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
            .collect()
    };
    assert!(names(&app).contains(&"Job/Projects".to_owned()));
    app.keys("u").await;
    app.wait_idle().await;
    let after_undo = names(&app);
    assert!(after_undo.contains(&"Work".to_owned()), "{after_undo:?}");
    assert!(after_undo.contains(&"Work/Projects".to_owned()), "{after_undo:?}");
    app.keys("<C-r>").await;
    app.wait_idle().await;
    let after_redo = names(&app);
    assert!(after_redo.contains(&"Job".to_owned()), "{after_redo:?}");
    assert!(after_redo.contains(&"Job/Projects".to_owned()), "{after_redo:?}");
}

#[tokio::test]
async fn folder_delete_undo_recreates_folders_and_items() {
    let mut app = ready_app().await;
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    let entries = lazybw::vault::filter::entries(&app.app.state.cache, &Default::default(), false);
    app.app.state.sidebar_row = entries.iter().position(|entry| entry.label == "Work").unwrap();
    app.keys("dd").await;
    app.keys(" ").await;
    app.keys("<CR>").await;
    app.wait_idle().await;
    let state = app.fake_state();
    let folder_names: Vec<String> = state["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
        .collect();
    assert!(!folder_names.iter().any(|name| name.starts_with("Work")), "{folder_names:?}");
    for item in state["items"].as_array().unwrap() {
        if item["id"] == "item-1" {
            assert!(item["folderId"].is_null(), "item kept its deleted folder");
        }
    }
    app.keys("u").await;
    app.wait_idle().await;
    let state = app.fake_state();
    let folders = state["folders"].as_array().unwrap();
    let work = folders.iter().find(|folder| folder["name"] == "Work").expect("Work recreated");
    let work_id = work["id"].as_str().unwrap();
    assert!(folders.iter().any(|folder| folder["name"] == "Work/Projects"));
    let github =
        state["items"].as_array().unwrap().iter().find(|item| item["id"] == "item-1").unwrap();
    assert_eq!(github["folderId"].as_str(), Some(work_id), "item was not moved back");
    app.keys("<C-r>").await;
    app.wait_idle().await;
    let state = app.fake_state();
    let folder_names: Vec<String> = state["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
        .collect();
    assert!(!folder_names.iter().any(|name| name.starts_with("Work")), "redo: {folder_names:?}");
}

#[tokio::test]
async fn editing_sends_the_full_item_and_preserves_unknown_fields() {
    let mut app = ready_app().await;
    let before = app.fixture_vault_item("item-1").expect("fixture item");
    app.keys("e").await;
    app.keys("iX<Esc>:wq<CR>").await;
    app.wait_idle().await;
    let after = app.fake_state()["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "item-1")
        .cloned()
        .expect("item after save");
    assert_eq!(after["name"], "XGitHub");
    fn without_nulls(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(key, value)| (key.clone(), without_nulls(value)))
                    .collect(),
            ),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(without_nulls).collect())
            }
            other => other.clone(),
        }
    }
    let before = without_nulls(&before);
    let after = without_nulls(&after);
    // Unknown fields at the item and nested login level survive the PUT.
    assert_eq!(after["futureTopLevel"], before["futureTopLevel"]);
    assert_eq!(after["login"]["futureLoginField"], 7);
    // Untouched fields are byte-identical after the round trip.
    for key in ["password", "totp", "username", "uris", "fido2Credentials", "passwordRevisionDate"]
    {
        assert_eq!(after["login"][key], before["login"][key], "login.{key} changed");
    }
    for key in ["fields", "notes", "favorite", "reprompt", "creationDate"] {
        assert_eq!(after[key], before[key], "{key} changed");
    }
    assert_eq!(after["folderId"], before["folderId"]);
    // The full item was sent: every key of the fixture exists in the body.
    fn contains_all(expected: &serde_json::Value, actual: &serde_json::Value) {
        match expected {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    let Some(found) = actual.get(key) else {
                        if value.is_null() {
                            continue;
                        }
                        panic!("missing key {key} in {actual}");
                    };
                    contains_all(value, found);
                }
            }
            other => assert_eq!(other, actual),
        }
    }
    contains_all(&before["login"], &after["login"]);
}

#[tokio::test]
async fn detail_yank_copies_the_field_value_not_its_label() {
    let mut app = ready_app().await;
    app.keys("<CR>").await;
    assert_eq!(app.app.state.selected_item().unwrap().name, "GitHub");
    app.keys("yy").await;
    assert_eq!(app.clipboard.contents().as_deref(), Some("dev-example"));
}

#[tokio::test]
async fn reprompt_runs_the_action_once_and_reports_a_wrong_password() {
    let mut app = ready_app().await;
    app.keys("<C-p>Passport<CR>").await;
    assert_eq!(app.app.state.selected_item().unwrap().name, "Passport");
    let fields = lazybw::app::update::detail_fields(&app.app.state);
    let row = fields.iter().position(|(key, _, _)| key == "id:passport").unwrap();
    app.app.state.detail.row = row;
    // Revealing a secret field asks for the master password first.
    app.keys("za").await;
    assert!(app.app.state.reprompt().is_some(), "za must ask for the master password");
    assert!(app.app.state.detail.revealed.is_empty());
    app.keys("<Esc>yy").await;
    assert!(app.app.state.reprompt().is_some());
    assert_eq!(app.clipboard.contents(), None);
    app.keys("wrong<CR>").await;
    app.wait_idle().await;
    let reprompt = app.app.state.reprompt().expect("the dialog stays open");
    assert!(reprompt.error.is_some(), "a wrong password is reported in the dialog");
    app.keys("correct horse<CR>").await;
    app.wait_idle().await;
    assert!(app.app.state.reprompt().is_none(), "no second prompt after the right password");
    assert_eq!(app.clipboard.contents().as_deref(), Some("X1234567"));
    // The re-prompt was consumed: the next secret copy asks again.
    app.keys("yy").await;
    assert!(app.app.state.reprompt().is_some());
}

#[tokio::test]
async fn export_checks_the_master_password() {
    let mut app = ready_app().await;
    app.keys("<Space>e").await;
    let destination = app.dir.path().join("export.json");
    {
        let Some(Overlay::Export(export)) = &mut app.app.state.overlay else {
            panic!("export wizard")
        };
        // This test is about the master password check: a plain export has
        // no file password step.
        export.format = lazybw::bw::service::ExportFormat::Json;
        export.path = destination.display().to_string();
    }
    app.keys("<CR>").await;
    assert_eq!(app.app.state.export().unwrap().step, 2);
    app.keys("not my password<CR>").await;
    app.wait_idle().await;
    let export = app.app.state.export().expect("the wizard stays open");
    assert!(export.error.is_some(), "a wrong master password must stop the export");
    assert!(!destination.exists(), "nothing is written with a wrong master password");
    app.keys("correct horse<CR>").await;
    app.wait_idle().await;
    assert!(app.app.state.export().is_none());
    assert!(destination.exists());
}

#[tokio::test]
async fn lock_drops_every_copy_of_vault_data() {
    let mut app = ready_app().await;
    app.keys("Y").await;
    assert!(app.app.state.yanked_item.is_some());
    app.keys("<Space>l").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.screen, Screen::Unlock);
    assert!(app.app.state.yanked_item.is_none());
    assert_eq!(app.app.state.cache.len(), 0);
}

#[tokio::test]
async fn import_wizard_filters_formats_and_keeps_folders() {
    let mut app = ready_app().await;
    app.keys("<Space>i").await;
    let import = app.app.state.import().expect("import wizard");
    assert_eq!(import.step, 0);
    assert_eq!(import.formats[0], "bitwardenjson", "Bitwarden formats come first");
    app.keys("keepass").await;
    let filtered: Vec<String> = app
        .app
        .state
        .import()
        .unwrap()
        .filtered_formats()
        .iter()
        .map(|f| (*f).to_owned())
        .collect();
    assert!(!filtered.is_empty() && filtered.iter().all(|f| f.contains("keepass")), "{filtered:?}");
    app.keys("<Esc>").await;
    assert!(app.app.state.import().is_none());

    // Skipping a duplicate keeps the file's folders and removes the
    // temporary copy afterwards.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("import.json");
    let body = serde_json::json!({
        "encrypted": false,
        "folders": [{"id": "f-travel", "name": "Travel"}],
        "items": [
            {"object": "item", "id": "a", "type": 1, "name": "GitHub", "folderId": null,
             "login": {"username": "dev-example", "password": "x",
                       "uris": [{"uri": "https://github.com", "match": null}]}},
            {"object": "item", "id": "b", "type": 1, "name": "Airline", "folderId": "f-travel",
             "login": {"username": "me@example.com", "password": "y"}}
        ]
    });
    std::fs::write(&path, serde_json::to_vec(&body).unwrap()).unwrap();
    let runtime = lazybw::security::secure_temp_dir().unwrap();
    let temp_files = || -> std::collections::BTreeSet<std::ffi::OsString> {
        std::fs::read_dir(&runtime)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .filter(|name| name.to_string_lossy().starts_with("import-"))
            .collect()
    };
    let before = temp_files();
    app.keys(&format!(":import bitwardenjson {}<CR>", path.display())).await;
    app.keys("<CR>").await;
    let import = app.app.state.import().unwrap();
    assert_eq!(import.step, 2);
    assert_eq!(import.new_folders, vec!["Travel".to_owned()]);
    assert_eq!(import.duplicates.len(), 1);
    app.keys("<CR>").await;
    app.wait_idle().await;
    assert!(app.app.state.import().is_none(), "{:?}", app.app.state.import().map(|i| &i.error));
    let folders: Vec<String> = app.fake_state()["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_owned))
        .collect();
    assert!(folders.contains(&"Travel".to_owned()), "{folders:?}");
    assert_message(&app, "Imported 1 items. Skipped 1 duplicate.");
    let leftovers: Vec<_> = temp_files().difference(&before).cloned().collect();
    assert!(leftovers.is_empty(), "filtered import file left behind: {leftovers:?}");
}

#[tokio::test]
async fn rail_keeps_the_cursor_visible_and_its_folder_icon_opens_the_picker() {
    let mut app = ready_app().await;
    app.app.state.size = (120, 36);
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    app.app.state.sidebar_row = 0;
    let state = &app.app.state;
    let entries = lazybw::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    );
    let slots = lazybw::vault::filter::rail_slots(&entries);
    let icons = slots.iter().filter(|slot| slot.selectable()).count();
    // Walk every icon: the cursor always sits on a drawn rail row, and the
    // folder icon is reached once.
    let mut on_folder = 0;
    for _ in 0..icons + 2 {
        app.keys("j").await;
        let state = &app.app.state;
        let position = lazybw::vault::filter::rail_position(&entries, &slots, state.sidebar_row);
        assert!(position.is_some(), "cursor on a hidden sidebar row {}", state.sidebar_row);
        if matches!(slots[position.unwrap()], lazybw::vault::filter::RailSlot::Folders(_)) {
            on_folder += 1;
        }
    }
    assert_eq!(on_folder, 1, "the folder icon is a single stop");
    // Back to the folder icon; Enter opens the folder picker.
    let folder_slot = slots
        .iter()
        .position(|slot| matches!(slot, lazybw::vault::filter::RailSlot::Folders(_)))
        .unwrap();
    app.app.state.sidebar_row = slots[folder_slot].entry().unwrap();
    app.keys("<CR>").await;
    let picker = app.app.state.picker().expect("folder picker");
    assert_eq!(picker.kind, lazybw::app::state::PickerKind::FolderJump);
    app.keys("Work/Projects<CR>").await;
    assert_eq!(app.app.state.filter, Filter::Folder("Work/Projects".to_owned()));
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::List);
    assert!(app.app.state.picker().is_none());
}

#[tokio::test]
async fn go_to_folder_opens_it_without_moving_the_item() {
    let mut app = ready_app().await;
    let before: Vec<_> =
        app.app.state.cache.active_items().map(|i| (i.id.clone(), i.folder_id.clone())).collect();
    app.keys("<Space>ff").await;
    assert_eq!(
        app.app.state.picker().map(|p| p.kind),
        Some(lazybw::app::state::PickerKind::FolderJump)
    );
    app.keys("Work/Projects<CR>").await;
    app.wait_idle().await;
    assert_eq!(app.app.state.filter, Filter::Folder("Work/Projects".to_owned()));
    let after: Vec<_> =
        app.app.state.cache.active_items().map(|i| (i.id.clone(), i.folder_id.clone())).collect();
    assert_eq!(before, after);
    assert!(!app.requests().iter().any(|entry| entry["request"]["method"] == "PUT"));
}

#[tokio::test]
async fn move_to_folder_from_the_picker_reaches_the_backend() {
    let mut app = ready_app().await;
    app.keys("m").await;
    assert_eq!(
        app.app.state.picker().map(|p| p.kind),
        Some(lazybw::app::state::PickerKind::Folder)
    );
    app.keys("Work/Projects<CR>").await;
    app.wait_idle().await;
    let put = app.requests().iter().any(|entry| entry["request"]["method"] == "PUT");
    assert!(put, "the move was never sent: {:?}", app.requests());
}

#[tokio::test]
async fn favorite_toggle_reaches_the_backend() {
    let mut app = ready_app().await;
    let id = app.app.state.selected_item().and_then(|item| item.id.clone()).expect("selection");
    let was = app.app.state.cache.item(&id).unwrap().favorite;
    app.keys("*").await;
    app.wait_idle().await;
    let stored = app.fake_state()["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id.as_str())
        .map(|item| item["favorite"].as_bool().unwrap_or(false))
        .unwrap();
    assert_eq!(stored, !was, "fake vault favourite unchanged: {:?}", app.requests());
}

#[tokio::test]
async fn za_from_the_list_reveals_the_password_instead_of_adding_an_item() {
    let mut app = ready_app().await;
    app.app.state.focus = lazybw::app::state::Focus::List;
    let item = app.app.state.selected_item().cloned().expect("selection");
    assert!(item.login.is_some(), "fixture starts on a login");
    app.keys("za").await;
    assert!(app.app.state.picker().is_none(), "za opened a picker");
    assert!(app.app.state.editor.is_none());
    // Either revealed, or (for a re-prompt item) asking for the password.
    assert!(
        app.app.state.detail.revealed.contains("password") || app.app.state.reprompt().is_some(),
        "password not revealed"
    );
    // An unknown z-sequence does nothing at all.
    app.keys("zk").await;
    assert!(app.app.state.picker().is_none());
}

#[tokio::test]
async fn folders_cannot_be_named_like_the_no_folder_filter() {
    let mut app = ready_app().await;
    app.keys(":folder new<CR>").await;
    assert!(app.app.state.prompt().is_some(), "folder name prompt");
    app.keys("No Folder<CR>").await;
    app.wait_idle().await;
    assert!(!app.requests().iter().any(|entry| entry["request"]["method"] == "POST"));
    assert!(!app.app.state.cache.folders().iter().any(|f| f.name == "No Folder"));
}

#[tokio::test]
async fn go_to_folder_lists_the_no_folder_filter_first() {
    let mut app = ready_app().await;
    app.keys("<Space>ff").await;
    let picker = app.app.state.picker().expect("picker");
    let first = &picker.entries[0];
    assert_eq!(first.label, "No folder");
    assert_eq!(first.value, "");
    assert_eq!(picker.entries.iter().filter(|e| e.value.is_empty()).count(), 1);
}

fn editor_row_value(app: &harness::TestApp, key: lazybw::app::state::FormKey) -> Option<String> {
    let editor = app.app.state.editor.as_ref()?;
    editor.rows.iter().find(|row| row.key == key).map(|row| row.value.clone())
}

#[tokio::test]
async fn ctrl_g_in_the_editor_generates_into_the_password_field() {
    use lazybw::app::state::FormKey;
    let mut app = ready_app().await;
    app.keys("al").await; // New item → login
    assert!(app.app.state.editor.is_some(), "new login form");
    // From the Name row: the popup opens and aims at Password.
    app.keys("<C-g>").await;
    app.wait_idle().await;
    assert!(app.app.state.generator_popup_open(), "generator popup");
    assert_eq!(app.app.state.contexts()[0], "generator");
    let generated = app.app.state.generator.output.clone();
    assert!(!generated.is_empty());
    // Options work inside the popup: `r` makes a new one.
    app.keys("r").await;
    app.wait_idle().await;
    let generated = app.app.state.generator.output.clone();
    app.keys("<CR>").await;
    assert!(!app.app.state.generator_popup_open());
    assert_eq!(editor_row_value(&app, FormKey::Password).as_deref(), Some(generated.as_str()));
    assert_eq!(editor_row_value(&app, FormKey::Name).as_deref(), Some(""));
    // Esc closes the popup without touching the field.
    app.keys("<C-g>").await;
    app.wait_idle().await;
    app.keys("<Esc>").await;
    assert!(!app.app.state.generator_popup_open());
    assert!(app.app.state.editor.is_some(), "Esc closed the popup, not the editor");
    assert_eq!(editor_row_value(&app, FormKey::Password).as_deref(), Some(generated.as_str()));
}

#[tokio::test]
async fn ctrl_e_off_the_notes_row_edits_the_notes_not_that_field() {
    use lazybw::app::state::FormKey;
    let editor = harness::workspace_file("scripts/fake-editor");
    let mut app = ready_app().await.with_editor(editor);
    app.keys("e").await; // GitHub
    let name = editor_row_value(&app, FormKey::Name);
    app.keys("<C-e>").await; // cursor on Name
    assert_eq!(editor_row_value(&app, FormKey::Name), name, "Name must be untouched");
    let editor = app.app.state.editor.as_ref().unwrap();
    assert_eq!(editor.rows[editor.row].key, FormKey::Notes);
}

#[tokio::test]
async fn closing_a_dirty_editor_offers_save_discard_and_cancel() {
    let mut app = ready_app().await;
    let before = app.fake_names().len();
    app.keys("al").await;
    app.keys("iThrowaway<Esc>").await;
    assert!(app.app.state.editor.as_ref().unwrap().dirty);
    // `:q` asks; `c` cancels and keeps the form.
    app.keys(":q<CR>").await;
    let confirm = app.app.state.confirm().expect("unsaved dialog");
    assert!(confirm.message.contains("Throwaway"), "{}", confirm.message);
    app.keys("c").await;
    assert!(app.app.state.confirm().is_none());
    assert!(app.app.state.editor.is_some());
    assert!(!app.app.state.quit);
    // `n` discards: the editor closes, lazybw keeps running, nothing saved.
    app.keys(":q<CR>").await;
    app.keys("n").await;
    app.wait_idle().await;
    assert!(app.app.state.editor.is_none());
    assert_eq!(app.app.state.screen, Screen::Vault);
    assert!(!app.app.state.quit);
    assert_eq!(app.fake_names().len(), before);
    // `:q!` discards without asking and does not quit lazybw.
    app.keys("al").await;
    app.keys("iOther<Esc>").await;
    app.keys(":q!<CR>").await;
    assert!(app.app.state.editor.is_none());
    assert!(!app.app.state.quit);
}

fn screen_text(state: &lazybw::app::AppState, width: u16, height: u16) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| lazybw::ui::draw(frame, state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[tokio::test]
async fn editor_action_rows_are_drawn_once_and_whole() {
    let mut app = ready_app().await;
    app.keys("al").await;
    let screen = screen_text(&app.app.state, 140, 40);
    assert!(screen.contains("+ Add website"), "{screen}");
    assert!(screen.contains("+ Add custom field"), "{screen}");
    assert!(!screen.contains("[ Add website ]"), "{screen}");
    assert!(!screen.contains("[ Add custom field ]"), "{screen}");
    // The generator popup draws over the form.
    app.keys("<C-g>").await;
    app.wait_idle().await;
    let screen = screen_text(&app.app.state, 140, 40);
    assert!(screen.contains("Generate password"), "{screen}");
    assert!(screen.contains("Enter use"), "{screen}");
    if std::env::var_os("LAZYBW_PRINT_SCREEN").is_some() {
        println!("{screen}");
    }
}

#[tokio::test]
async fn qa_quits_lazybw_even_from_the_editor() {
    let mut app = ready_app().await;
    app.keys("al").await;
    app.keys(":qa!<CR>").await;
    assert!(app.app.state.quit);
}

#[tokio::test]
async fn editor_j_walks_each_column_and_h_l_switch_columns() {
    use lazybw::app::state::FormKey;
    let mut app = ready_app().await;
    app.app.state.size = (180, 45);
    app.keys("al").await;
    let key = |app: &harness::TestApp| {
        let editor = app.app.state.editor.as_ref().unwrap();
        editor.rows[editor.row].key
    };
    // j visits the whole left column before the right one: never back.
    let mut seen_right = false;
    let total = app.app.state.editor.as_ref().unwrap().rows.len();
    for _ in 0..total {
        let right = key(&app).in_right_column();
        assert!(!(seen_right && !right), "j went back to the left column at {:?}", key(&app));
        seen_right |= right;
        app.keys("j").await;
    }
    assert!(seen_right);
    // gg, then l to the right column and h back to the same row.
    app.keys("gg").await;
    assert_eq!(key(&app), FormKey::Name);
    app.keys("l").await;
    assert!(key(&app).in_right_column(), "{:?}", key(&app));
    app.keys("h").await;
    assert_eq!(key(&app), FormKey::Name);
    // h in the left column stays put; the vault focus is untouched.
    let focus = app.app.state.focus;
    app.keys("h").await;
    assert_eq!(key(&app), FormKey::Name);
    assert_eq!(app.app.state.focus, focus);
}

#[tokio::test]
async fn sidebar_h_and_l_fold_folders_like_a_file_tree() {
    let mut app = ready_app().await;
    let entries = |app: &harness::TestApp| {
        let state = &app.app.state;
        lazybw::vault::filter::entries(
            &state.cache,
            &state.collapsed,
            state.config.vault.show_empty_types,
        )
    };
    let row_of = |app: &harness::TestApp, label: &str| {
        entries(app).iter().position(|e| e.label == label).expect(label)
    };
    let projects_shown =
        |app: &harness::TestApp| entries(app).iter().any(|e| e.label == "Projects");
    assert!(projects_shown(&app), "Work starts expanded");
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    // From the subfolder, h goes up to its parent.
    app.app.state.sidebar_row = row_of(&app, "Projects");
    app.keys("h").await;
    assert_eq!(app.app.state.sidebar_row, row_of(&app, "Work"));
    // h closes the parent, l opens it again, a second l shows its items.
    app.keys("h").await;
    assert!(!projects_shown(&app), "h closed Work");
    app.keys("l").await;
    assert!(projects_shown(&app), "l opened Work");
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::Sidebar);
    app.keys("l").await;
    assert_eq!(app.app.state.filter, Filter::Folder("Work".to_owned()));
    assert_eq!(app.app.state.focus, lazybw::app::state::Focus::List);
}

#[tokio::test]
async fn clicking_a_fold_arrow_toggles_the_folder() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut app = ready_app().await;
    let plan = lazybw::ui::layout::LayoutPlan::compute_sized(
        ratatui::layout::Rect::new(0, 0, app.app.state.size.0, app.app.state.size.1),
        app.app.state.sidebar_visible,
        app.app.state.sidebar_width,
        app.app.state.list_width,
    );
    let state = &app.app.state;
    let entries = lazybw::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    );
    let work = entries.iter().position(|e| e.label == "Work").unwrap() as u16;
    let filter = state.filter.clone();
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    app.handle(lazybw::app::Msg::Mouse(click(plan.sidebar.x + 3, plan.sidebar.y + 1 + work))).await;
    assert!(app.app.state.collapsed.contains("Work"), "arrow click closed Work");
    assert_eq!(app.app.state.filter, filter, "the arrow does not filter");
    app.handle(lazybw::app::Msg::Mouse(click(plan.sidebar.x + 3, plan.sidebar.y + 1 + work))).await;
    assert!(!app.app.state.collapsed.contains("Work"));
}

#[tokio::test]
async fn collapsed_folders_survive_a_restart_without_writing_names() {
    use std::os::unix::fs::PermissionsExt;
    let mut app = ready_app().await;
    let path = app.app.state.paths.folds_file();
    let state = &app.app.state;
    let work = lazybw::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    )
    .iter()
    .position(|e| e.label == "Work")
    .unwrap();
    app.app.state.focus = lazybw::app::state::Focus::Sidebar;
    app.app.state.sidebar_row = work;
    app.keys("h").await;
    assert!(app.app.state.collapsed.contains("Work"));
    let text = std::fs::read_to_string(&path).expect("folds file written");
    assert!(!text.contains("Work"), "folder names must not reach the disk: {text}");
    let work_id = app
        .app
        .state
        .cache
        .folders()
        .iter()
        .find(|f| f.name == "Work")
        .and_then(|f| f.id.clone())
        .unwrap();
    assert!(text.contains(work_id.as_str()), "{text}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    // "Restart": forget the in-memory state, load the vault again.
    app.app.state.collapsed.clear();
    app.app.state.folds_restored = false;
    app.keys("R").await;
    app.wait_idle().await;
    assert!(app.app.state.collapsed.contains("Work"), "restored from {text}");
    // Opening it again is saved too.
    app.keys("l").await;
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains(work_id.as_str()), "{text}");
}

#[tokio::test]
async fn editor_u_and_ctrl_r_undo_whole_insert_sessions() {
    use lazybw::app::state::FormKey;
    let mut app = ready_app().await;
    app.keys("e").await; // GitHub
    let name = editor_row_value(&app, FormKey::Name).unwrap();
    assert!(!app.app.state.editor.as_ref().unwrap().dirty);
    // One INSERT session = one step, however many characters.
    app.keys("A edited<Esc>").await;
    assert_eq!(editor_row_value(&app, FormKey::Name).unwrap(), format!("{name} edited"));
    app.keys("jj").await; // Name → Folder → Username
    app.keys("ccalice<Esc>").await;
    assert_eq!(editor_row_value(&app, FormKey::Username).as_deref(), Some("alice"));
    app.keys("u").await;
    assert_ne!(editor_row_value(&app, FormKey::Username).as_deref(), Some("alice"));
    assert_eq!(editor_row_value(&app, FormKey::Name).unwrap(), format!("{name} edited"));
    app.keys("u").await;
    assert_eq!(editor_row_value(&app, FormKey::Name).unwrap(), name);
    assert!(!app.app.state.editor.as_ref().unwrap().dirty, "back at the saved state");
    app.keys("<C-r>").await;
    assert_eq!(editor_row_value(&app, FormKey::Name).unwrap(), format!("{name} edited"));
    assert!(app.app.state.editor.as_ref().unwrap().dirty);
    // Saving makes this the clean state; undo past it is dirty again.
    app.keys(":w<CR>").await;
    app.wait_idle().await;
    assert!(!app.app.state.editor.as_ref().unwrap().dirty);
    app.keys("u").await;
    assert_eq!(editor_row_value(&app, FormKey::Name).unwrap(), name);
    assert!(app.app.state.editor.as_ref().unwrap().dirty);
}

#[tokio::test]
async fn theme_picker_previews_live_and_esc_restores() {
    let mut app = ready_app().await;
    let start = app.app.state.config.ui.theme.clone();
    app.keys("<Space>t").await;
    let picker = app.app.state.picker().expect("theme picker");
    assert_eq!(
        picker.selected().map(|e| e.value.clone()),
        Some(start.clone()),
        "starts on current"
    );
    app.keys("<Down>").await;
    let previewed = app.app.state.theme.name.clone();
    assert_ne!(previewed, start, "moving previews the next theme");
    assert_eq!(app.app.state.config.ui.theme, start, "config untouched while previewing");
    app.keys("<Esc>").await;
    assert_eq!(app.app.state.theme.name, start, "Esc restores");
    // Enter keeps the highlighted theme.
    app.keys("<Space>t").await;
    app.keys("<Down><CR>").await;
    assert_eq!(app.app.state.theme.name, previewed);
    assert_eq!(app.app.state.config.ui.theme, previewed);
}

#[tokio::test]
async fn the_export_wizard_suggests_a_password_protected_file() {
    let mut app = ready_app().await;
    app.keys("<Space>e").await;
    assert_eq!(
        app.app.state.export().unwrap().format,
        lazybw::bw::service::ExportFormat::PasswordProtectedJson
    );
    app.keys("<Esc>").await;
    app.keys(":export json /tmp/never-written.json<CR>").await;
    assert_eq!(
        app.app.state.export().unwrap().format,
        lazybw::bw::service::ExportFormat::Json,
        "an explicit format is respected"
    );
}

#[tokio::test]
async fn pickers_move_with_ctrl_n_and_ctrl_p() {
    let mut app = ready_app().await;
    app.keys("m").await;
    let index = |app: &harness::TestApp| app.app.state.picker().expect("picker").index;
    assert_eq!(index(&app), 0);
    app.keys("<C-n><C-n>").await;
    assert_eq!(index(&app), 2);
    app.keys("<C-p>").await;
    assert_eq!(index(&app), 1);
    app.keys("<Down>").await;
    assert_eq!(index(&app), 2);
    app.keys("<Up>").await;
    assert_eq!(index(&app), 1);
}

#[tokio::test]
async fn settings_move_with_ctrl_n_and_ctrl_p() {
    let mut app = ready_app().await;
    app.keys(" ,").await;
    assert_eq!(app.app.state.screen, lazybw::app::state::Screen::Settings);
    let before = app.app.state.settings.index;
    app.keys("<C-n><C-n>").await;
    assert_eq!(app.app.state.settings.index, before + 2);
    app.keys("<C-p>").await;
    assert_eq!(app.app.state.settings.index, before + 1);
    assert!(app.app.state.quick_open().is_none(), "Ctrl-p opened quick open");
}
