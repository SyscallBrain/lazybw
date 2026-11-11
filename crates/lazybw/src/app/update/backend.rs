//! Backend results and resumed terminal tasks.

use super::{
    AppState, CommandKind, Effect, Filter, FormKey, Instant, Item, MessageLevel, Overlay, Pending,
    Screen, SessionPhase, SortOrder, Suspended, UnlockState, animate, apply, drive_folder_restore,
    next_id, ordered_import_formats, rebuild_rows, request, run, show_backend_error, start_login,
};
use crate::app::actions::ActionId;

pub(crate) fn on_backend(
    state: &mut AppState,
    event: crate::bw::service::BackendEvent,
) -> Vec<Effect> {
    let id = event.id;
    let pending = state.pending.remove(&id);
    let mut effects = Vec::new();
    use crate::bw::service::{BwEvent, FolderAction};
    // A response with vault data whose request was forgotten (the vault was
    // locked or logged out since) must not repopulate the cache or reopen
    // the vault on screen.
    let carries_vault_data = matches!(
        event.event,
        BwEvent::Vault(_)
            | BwEvent::Item(_)
            | BwEvent::ItemWritten(_)
            | BwEvent::ItemDeleted(_)
            | BwEvent::ItemRestored(_)
            | BwEvent::Folder(_)
            | BwEvent::Health(_)
            | BwEvent::Generated(_)
    );
    let stale = pending.is_none()
        || (matches!(event.event, BwEvent::Vault(_))
            && !matches!(state.session, SessionPhase::Loading | SessionPhase::Ready));
    if carries_vault_data && stale {
        tracing::debug!(id, "dropping a response that arrived after a lock");
        return effects;
    }
    // A change failed while `q` was waiting for it: stay, so the error is
    // read instead of flashing by on the way out.
    if state.quit_when_saved && event.event.error().is_some() {
        state.quit_when_saved = false;
    }
    match event.event {
        BwEvent::Started(result) => {
            on_started(state, result, &mut effects);
            // A master password typed ahead on the start screen
            // (`starting_key`) is only for a locked vault, and was used just
            // now if Enter was pressed; never keep it for another screen.
            if state.session != SessionPhase::Locked {
                state.unlock = UnlockState::default();
            } else if std::mem::take(&mut state.unlock.submit_when_ready) {
                effects
                    .extend(apply(state, crate::app::state::Action::new(ActionId::UnlockSubmit)));
            }
        }
        BwEvent::SessionSet(result) => match result {
            Ok(()) => {
                let id = next_id(state);
                state.pending.insert(id, Pending::Starting);
                effects.push(Effect::StartBackend { id });
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::Unlocked(result) if matches!(pending, Some(Pending::Reprompt)) => {
            // Only the request recorded in the open dialog counts: the
            // dialog may have been closed, or replaced by another, since.
            if state.reprompt().and_then(|reprompt| reprompt.request) != Some(id) {
                return effects;
            }
            match result {
                Ok(_) => {
                    let Some(reprompt) = state.take_reprompt() else { return effects };
                    if let Some(grace) = state.config.security.reprompt_grace.get()
                        && !grace.is_zero()
                    {
                        state.reprompt_passed =
                            Some((reprompt.item_id.clone(), Instant::now() + grace));
                    }
                    state.reprompt_once = Some(reprompt.item_id);
                    effects.extend(run(state, reprompt.action));
                    state.reprompt_once = None;
                }
                Err(err) => {
                    // A wrong password keeps the dialog open with the error;
                    // the vault itself stays unlocked.
                    if let Some(Overlay::Reprompt(reprompt)) = &mut state.overlay {
                        reprompt.request = None;
                        reprompt.error = Some(err.to_string());
                    }
                    if !matches!(err, crate::bw::BwError::InvalidPassword) {
                        show_backend_error(state, &err);
                    }
                }
            }
        }
        BwEvent::Unlocked(result) => match result {
            Ok(()) => {
                state.unlock = UnlockState::default();
                state.enter_session(SessionPhase::Loading);
                state.toast(MessageLevel::Ok, "Vault unlocked.");
                effects.push(request(state, CommandKind::LoadVault, Pending::Load));
            }
            Err(err) => {
                if matches!(err, crate::bw::BwError::InvalidPassword) {
                    state.unlock.error = Some(err.to_string());
                    state.unlock.shake = Some(Instant::now());
                    state.unlock.password.clear();
                    animate(state, crate::ui::fx::FxKind::Shake);
                } else {
                    show_backend_error(state, &err);
                }
            }
        },
        BwEvent::Locked(result) => match result {
            Ok(()) => {
                state.drop_secrets();
                state.toast(MessageLevel::Ok, "Vault locked.");
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::Synced(result) => match result {
            Ok(status) => {
                state.last_sync = status
                    .last_sync
                    .as_deref()
                    .and_then(crate::util::time::parse_iso)
                    .or(Some(state.now));
                state.message(MessageLevel::Info, "Synced. Reloading…");
                effects.push(request(state, CommandKind::LoadVault, Pending::Load));
            }
            Err(err) => {
                state.toast_with(MessageLevel::Error, "Sync failed.", err.to_string());
            }
        },
        BwEvent::Vault(result) => match result {
            Ok(snapshot) => {
                if !state.folds_restored {
                    effects.push(Effect::LoadFolds);
                }
                let count = snapshot.items.len();
                let previous_revision = state
                    .editor
                    .as_ref()
                    .and_then(|editor| editor.item.id.clone())
                    .and_then(|id| state.cache.item(&id).cloned())
                    .and_then(|item| item.revision_date);
                // The panes sweep in when the vault opens, not on every
                // reload (a background sync, a folder change).
                let opening = state.session == SessionPhase::Loading;
                // Keeps the cursor on the item it was on.
                state.apply_snapshot(snapshot);
                state.last_sync_at = Some(Instant::now());
                if opening {
                    animate(state, crate::ui::fx::FxKind::Sweep);
                }
                let current_revision =
                    state.editor.as_ref().and_then(|editor| editor.item.id.clone()).and_then(
                        |id| state.cache.item(&id).and_then(|item| item.revision_date.clone()),
                    );
                if let (Some(current), Some(previous)) = (current_revision, previous_revision)
                    && current != previous
                    && let Some(editor) = &mut state.editor
                {
                    editor.remote_change = true;
                }
                state.message(
                    MessageLevel::Ok,
                    format!("Synced {}.", crate::util::text::items(count)),
                );
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::Item(result) => match result {
            Ok(item) => {
                // `:e!`: the editor shows the server's copy, its own changes
                // thrown away, as in Vim.
                let reload = matches!(pending, Some(Pending::GetItem))
                    && state.editor.as_ref().is_some_and(|e| e.item.id == item.id);
                state.cache.upsert(item.clone());
                if reload {
                    let row = state.editor.as_ref().map_or(0, |e| e.row);
                    let name = item.name.clone();
                    state.notes = None;
                    super::open_editor(state, item, false);
                    if let Some(editor) = &mut state.editor {
                        editor.row = row.min(editor.rows.len().saturating_sub(1));
                    }
                    state.message(MessageLevel::Ok, format!("Reloaded “{name}” from Bitwarden."));
                }
                state.clamp_selection();
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::ItemWritten(result) => match result {
            Ok(item) => {
                let name = item.name.clone();
                let (sent_id, close, sent_editor) = match &pending {
                    Some(Pending::Save { id, close, editor }) => (id.clone(), *close, *editor),
                    _ => (None, false, None),
                };
                let was_new = sent_id.is_none();
                // Only a save sent by the editor may touch the editor, and
                // only while that same editor instance is open.
                let editor_is_target = sent_editor.is_some_and(|sent| {
                    state
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.instance_id == sent.instance_id)
                });
                let before = if editor_is_target && !was_new {
                    state.editor.as_ref().map(|editor| editor.original_item.clone())
                } else {
                    None
                };
                state.cache.upsert(item.clone());
                if editor_is_target && let Some(editor) = &mut state.editor {
                    let unchanged =
                        sent_editor.is_some_and(|sent| editor.history.revision == sent.revision);
                    if unchanged {
                        if close {
                            state.editor = None;
                            state.screen = Screen::Vault;
                        } else {
                            editor.item = item.clone();
                            editor.original_item = item.clone();
                            editor.dirty = false;
                            editor.history.saved_depth = editor.history.undo.len();
                            // The server's copy is not a user edit.
                            editor.history.restoring = true;
                            editor.is_new = false;
                            rebuild_rows(state);
                        }
                    } else {
                        // Edits made after the save was sent stay in the form;
                        // take only what the next save needs from the server.
                        editor.item.id = item.id.clone();
                        editor.item.revision_date = item.revision_date.clone();
                        editor.original_item = item.clone();
                        editor.is_new = false;
                        editor.dirty = true;
                        if close {
                            state.message(
                                MessageLevel::Warn,
                                "Saved, but the editor has newer changes; it stays open.",
                            );
                        }
                    }
                }
                if item.id.is_some() {
                    if was_new {
                        state.undo.push(crate::app::state::UndoOp::Create {
                            item: Box::new(item.clone()),
                        });
                        state.toast(MessageLevel::Ok, format!("Created “{name}”."));
                    } else {
                        if let (Some(before), Some(id)) = (before, item.id.clone()) {
                            // A save from the editor is one undo step in the vault.
                            state.undo.push(crate::app::state::UndoOp::Edit {
                                id,
                                before: Box::new(before),
                                after: Box::new(item.clone()),
                            });
                        }
                        state.toast(MessageLevel::Ok, format!("Saved “{name}”."));
                    }
                }
                state.clamp_selection();
            }
            Err(err) => {
                let (id, from_editor) = match &pending {
                    Some(Pending::Save { id, editor, .. }) => (id.clone(), editor.is_some()),
                    _ => (None, false),
                };
                if from_editor {
                    // The form still holds the changes; nothing was assumed.
                    state.toast_with(
                        MessageLevel::Error,
                        "Couldn't save.",
                        format!("{err}. Your changes are still in the editor."),
                    );
                } else {
                    effects.extend(rollback_failed_change(state, id.as_deref(), &err));
                }
            }
        },
        BwEvent::ItemDeleted(result) => match result {
            Ok(id) => {
                let permanent = matches!(pending, Some(Pending::Delete { permanent: true, .. }));
                if let Some(mut item) = state.cache.remove(&id)
                    && !permanent
                {
                    item.deleted_date = Some(state.now.to_rfc3339());
                    state.cache.upsert(item);
                }
                state.clamp_selection();
            }
            Err(err) => {
                let id = match &pending {
                    Some(Pending::Delete { id, .. }) => Some(id.clone()),
                    _ => None,
                };
                effects.extend(rollback_failed_change(state, id.as_deref(), &err));
            }
        },
        BwEvent::ItemRestored(result) => match result {
            Ok(id) => {
                if let Some(mut item) = state.cache.remove(&id) {
                    item.deleted_date = None;
                    state.cache.upsert(item);
                }
                state.clamp_selection();
            }
            Err(err) => {
                let id = match &pending {
                    Some(Pending::Restore { id }) => Some(id.clone()),
                    _ => None,
                };
                effects.extend(rollback_failed_change(state, id.as_deref(), &err));
            }
        },
        BwEvent::Folder(result) => match result {
            Ok(action) => {
                match &action {
                    FolderAction::Created(folder) => {
                        let is_restore = state.folder_restore.as_ref().is_some_and(|restore| {
                            restore.pending.as_deref() == Some(folder.name.as_str())
                        });
                        if is_restore {
                            if let Some(id) = folder.id.clone()
                                && let Some(restore) = &mut state.folder_restore
                            {
                                restore.created.insert(folder.name.clone(), id);
                                restore.pending = None;
                            }
                            effects.extend(drive_folder_restore(state));
                            state.toast(
                                MessageLevel::Ok,
                                format!("Recreated folder “{}”.", folder.name),
                            );
                        } else {
                            state.toast(
                                MessageLevel::Ok,
                                format!("Created folder “{}”.", folder.name),
                            );
                            state.undo.push(crate::app::state::UndoOp::FolderCreate {
                                name: folder.name.clone(),
                            });
                        }
                    }
                    FolderAction::Renamed(folder) => {
                        state.toast(
                            MessageLevel::Ok,
                            format!("Renamed folder to “{}”.", folder.name),
                        );
                    }
                    FolderAction::Deleted(_) => {
                        state.toast(
                            MessageLevel::Ok,
                            "Deleted folder. Its items moved to No folder.",
                        );
                    }
                }
                effects.push(request(state, CommandKind::LoadVault, Pending::Load));
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::Generated(result) => match result {
            Ok(password) => {
                let (record, copy, target) = match pending {
                    Some(Pending::Generate { record, copy, target }) => (record, copy, target),
                    _ => (true, false, None),
                };
                state.generator.output = password.clone();
                state.generator.error = None;
                if record {
                    state.generator.history.insert(0, (password.clone(), Instant::now()));
                    state.generator.history.truncate(50);
                }
                if copy {
                    effects.extend(super::copy_value(
                        state,
                        &password,
                        "password",
                        "the generator",
                    ));
                }
                if let Some(id) = target
                    && let Some(item) = state.cache.item(&id).cloned()
                {
                    let mut updated = item.clone();
                    if let Some(login) = &mut updated.login {
                        login.password = Some(crate::bw::sensitive::Sensitive::new(password));
                    }
                    state.undo.push(crate::app::state::UndoOp::Edit {
                        id: id.clone(),
                        before: Box::new(item),
                        after: Box::new(updated.clone()),
                    });
                    effects.push(request(
                        state,
                        CommandKind::PutItem { id: id.clone(), item: Box::new(updated) },
                        Pending::Save { id: Some(id.clone()), close: false, editor: None },
                    ));
                }
            }
            Err(err) => {
                state.generator.error = Some(err.to_string());
            }
        },
        BwEvent::Exported(result) => match result {
            Ok(outcome) => {
                let path = outcome.path.display().to_string();
                let count = state.cache.active_items().count();
                state.toast(
                    MessageLevel::Ok,
                    format!("Exported {} to {path}.", crate::util::text::items(count)),
                );
                if state.export().is_some_and(|export| export.format.is_plaintext()) {
                    state.message(
                        MessageLevel::Warn,
                        "This file contains every password in plain text. Keep it safe and delete it when done.",
                    );
                }
                // Only the wizard closes: it may have been dismissed while
                // the CLI worked, and the user has moved on since.
                state.take_export();
            }
            Err(err) => {
                if let Some(Overlay::Export(export)) = &mut state.overlay {
                    export.step = if matches!(err, crate::bw::BwError::InvalidPassword) {
                        // Ask for the master password again.
                        crate::app::state::ExportStep::MasterPassword
                    } else {
                        crate::app::state::ExportStep::Format
                    };
                    export.error = Some(err.to_string());
                } else {
                    show_backend_error(state, &err);
                }
            }
        },
        BwEvent::ImportFormats(formats) => {
            if let Some(Overlay::Import(import)) = &mut state.overlay {
                let selected =
                    import.filtered_formats().get(import.format_index).map(|f| (*f).to_owned());
                import.formats = ordered_import_formats(&formats);
                import.format_index = selected
                    .and_then(|f| import.filtered_formats().iter().position(|x| *x == f))
                    .unwrap_or(0);
            }
        }
        BwEvent::Imported(result) => match result {
            Ok(message) => {
                let text = match state.last_import.take() {
                    Some((imported, skipped)) if skipped > 0 => format!(
                        "Imported {}. Skipped {skipped} duplicate{}.",
                        crate::util::text::items(imported),
                        if skipped == 1 { "" } else { "s" }
                    ),
                    Some((imported, _)) => {
                        format!("Imported {}.", crate::util::text::items(imported))
                    }
                    None => message,
                };
                state.toast(MessageLevel::Ok, text);
                state.take_import();
                state.screen = Screen::Vault;
                // Show what arrived: All items, newest first.
                state.filter = Filter::All;
                state.config.vault.sort = SortOrder::Created;
                state.sort_reversed = false;
                state.list_row = 0;
                state.focus = crate::app::state::Focus::List;
                // The import ran in a separate `bw` process: sync so `bw serve`
                // sees the new items, then reload.
                effects.push(request(state, CommandKind::Sync { force: true }, Pending::Sync));
            }
            Err(err) => {
                state.last_import = None;
                if let Some(Overlay::Import(import)) = &mut state.overlay {
                    import.error = Some(err.to_string());
                } else {
                    show_backend_error(state, &err);
                }
            }
        },
        BwEvent::Health(result) => match result {
            Ok(entries) => {
                let old_age = state
                    .config
                    .health
                    .old_password_age
                    .seconds()
                    .map(|s| s / 86_400)
                    .unwrap_or(365);
                let local = !state.capabilities.as_ref().is_some_and(|caps| caps.password_health);
                let report = crate::vault::health::build(
                    &state.cache.active_items().collect::<Vec<_>>(),
                    &entries,
                    state.now,
                    old_age,
                    local,
                );
                state.health.raw = entries;
                state.cache.apply_health(&report);
                state.health_epoch += 1;
                let message = format!(
                    "Checked {} logins. {} exposed, {} reused, {} weak.",
                    report.checked, report.exposed, report.reused, report.weak
                );
                state.health.report = Some(report);
                state.toast(MessageLevel::Ok, message);
            }
            Err(err) => {
                state.toast_with(MessageLevel::Error, "Health check failed.", err.to_string());
            }
        },
        BwEvent::LoggedIn(result) => match result {
            Ok(status) => {
                state.login = Default::default();
                state.email = status.user_email.clone();
                state.server = status.server_url.clone();
                // `bw login` syncs; the vault load no longer asks for this.
                state.last_sync =
                    status.last_sync.as_deref().and_then(crate::util::time::parse_iso);
                state.enter_session(SessionPhase::Locked);
                state.toast(MessageLevel::Ok, "Logged in. Unlock the vault to continue.");
            }
            Err(err) => state.login.error = Some(err.to_string()),
        },
        BwEvent::LoggedOut(result) => match result {
            Ok(()) => {
                // Clear everything, even when logging out from the unlock
                // screen where nothing should be left.
                state.forget_vault();
                state.enter_session(SessionPhase::LoggedOut);
                state.email = None;
                state.toast(MessageLevel::Ok, "Logged out.");
            }
            Err(err) => show_backend_error(state, &err),
        },
        BwEvent::ServerConfigured(result) => {
            let url = state.login.configuring_server.take();
            let login = std::mem::take(&mut state.login.login_after_server);
            match result {
                Ok(()) => {
                    state.login.configured_server = url;
                    if login {
                        // Enter on the method list was waiting for this.
                        effects.extend(start_login(state));
                    } else {
                        state.message(MessageLevel::Ok, "Server saved.");
                    }
                }
                Err(err) => state.login.error = Some(err.to_string()),
            }
        }
        BwEvent::Fatal(message) => {
            // Leaving Ready forgets the vault: a retry starts a new CLI that
            // has to be unlocked again anyway.
            state.enter_session(SessionPhase::Failed(Box::new(crate::app::state::ErrorScreen {
                title: "Bitwarden CLI stopped".to_owned(),
                lines: vec![message],
                detail: Vec::new(),
                actions: "r retry   q quit".to_owned(),
                retry: true,
                open_file: None,
            })));
        }
    }
    effects
}

pub(crate) fn on_suspended(state: &mut AppState, result: Suspended) -> Vec<Effect> {
    match result {
        Suspended::Login(Some(key)) => vec![Effect::SetSession(key)],
        Suspended::Login(None) => {
            state.login.error = Some("Login failed.".to_owned());
            Vec::new()
        }
        Suspended::LoginFailed => Vec::new(),
        Suspended::Notes(text) => {
            if let Some(notes) = &state.notes {
                notes.borrow_mut().editor =
                    edtui::EditorState::new(edtui::Lines::from(text.expose()));
            }
            if let Some(editor) = &mut state.editor {
                // An editor closed without changes leaves the item clean.
                let before = editor.item.notes.as_ref().map_or("", |n| n.expose());
                if text.expose().trim_end_matches('\n') != before.trim_end_matches('\n') {
                    crate::app::form::set(
                        &mut editor.item,
                        FormKey::Notes,
                        text.expose(),
                        &state.cache,
                    );
                    editor.dirty = true;
                }
            }
            Vec::new()
        }
        Suspended::SshPrivateKey(text) => {
            if text.expose().trim().is_empty() {
                return Vec::new();
            }
            match crate::util::ssh::parse_private_key(text.expose()) {
                Ok(key) => {
                    if let Some(editor) = &mut state.editor {
                        let ssh = editor.item.ssh_key.get_or_insert_with(Default::default);
                        ssh.private_key =
                            Some(crate::bw::sensitive::Sensitive::new(key.private_key));
                        ssh.public_key = key.public_key;
                        ssh.key_fingerprint = key.fingerprint;
                        editor.dirty = true;
                    }
                    rebuild_rows(state);
                    state.message(MessageLevel::Ok, "Updated the SSH key.");
                }
                Err(err) => state.toast_with(
                    MessageLevel::Error,
                    "That is not a valid SSH private key.",
                    err.to_string(),
                ),
            }
            Vec::new()
        }
        Suspended::ItemJson(text) => {
            match serde_json::from_str::<Item>(text.expose()) {
                Ok(item) => {
                    if let Some(editor) = &mut state.editor {
                        if editor.item != item {
                            editor.item = item;
                            editor.dirty = true;
                            rebuild_rows(state);
                        }
                    } else {
                        // `gE` from the list: the edit opens in the editor,
                        // where it is reviewed and saved like any other.
                        let Some(original) =
                            item.id.as_deref().and_then(|id| state.cache.item(id)).cloned()
                        else {
                            state.toast(
                                MessageLevel::Error,
                                "The JSON must keep the item's id; nothing was changed.",
                            );
                            return Vec::new();
                        };
                        if original == item {
                            state.message(MessageLevel::Info, "No changes.");
                            return Vec::new();
                        }
                        super::open_editor(state, original, false);
                        if let Some(editor) = &mut state.editor {
                            editor.item = item;
                            editor.dirty = true;
                        }
                        rebuild_rows(state);
                        state.message(
                            MessageLevel::Info,
                            "Review the changes, then Ctrl-s saves or ZQ discards them.",
                        );
                    }
                }
                Err(err) => {
                    // serde's message can quote a value from the JSON (a
                    // password); report only where the problem is.
                    state.toast_with(
                        MessageLevel::Error,
                        "The JSON is not a valid item.",
                        format!(
                            "{:?} error at line {}, column {}.",
                            err.classify(),
                            err.line(),
                            err.column()
                        ),
                    );
                }
            }
            Vec::new()
        }
        Suspended::Failed(err) => {
            state.toast_with(MessageLevel::Error, "The editor failed.", err);
            Vec::new()
        }
        Suspended::Cancelled => Vec::new(),
    }
}

/// A change that was applied to the cache before the server answered (trash,
/// favourite, move, undo) failed. Its undo entry describes something that
/// did not happen, so it goes; the vault is reloaded so the list matches the
/// server again.
fn rollback_failed_change(
    state: &mut AppState,
    id: Option<&str>,
    err: &crate::bw::BwError,
) -> Vec<Effect> {
    if let Some(id) = id {
        state.undo.forget(id);
    }
    show_backend_error(state, err);
    if !state.session.is_ready() || state.pending.values().any(|p| matches!(p, Pending::Load)) {
        return Vec::new();
    }
    state.message(
        MessageLevel::Warn,
        "A change failed; reloading the vault to show its real state.",
    );
    vec![request(state, CommandKind::LoadVault, Pending::Load)]
}

/// The CLI answered (or failed) at start: pick the session phase.
fn on_started(
    state: &mut AppState,
    result: Result<crate::bw::models::Status, crate::bw::BwError>,
    effects: &mut Vec<Effect>,
) {
    match result {
        Ok(status) => {
            state.email = status.user_email.clone();
            state.server = status.server_url.clone();
            state.last_sync = status.last_sync.as_deref().and_then(crate::util::time::parse_iso);
            if status.is_unlocked() {
                state.enter_session(SessionPhase::Loading);
                effects.push(request(state, CommandKind::LoadVault, Pending::Load));
            } else if status.is_locked() {
                state.enter_session(SessionPhase::Locked);
            } else {
                state.enter_session(SessionPhase::LoggedOut);
            }
        }
        // No account yet: `bw serve` refuses to run until one logs in.
        Err(err) if err.requires_login() => state.enter_session(SessionPhase::LoggedOut),
        Err(err) => {
            // Without a running CLI nothing else can happen: say so and
            // offer a retry instead of spinning on the start screen.
            state.enter_session(SessionPhase::Failed(Box::new(crate::app::state::ErrorScreen {
                title: "Couldn't start the Bitwarden CLI".to_owned(),
                lines: vec![err.to_string()],
                detail: Vec::new(),
                actions: "r retry   q quit".to_owned(),
                retry: true,
                open_file: None,
            })));
        }
    }
}
