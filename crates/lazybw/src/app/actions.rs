//! The action registry: one source of truth for keys, `:` commands, which-key,
//! the help screen, the keymap editor and `lazybw keys`.
//!
//! Every user-invokable behaviour is an [`ActionDef`] with a stable id
//! (`domain.verb`), a description used verbatim in the UI, the contexts where
//! it is available and its argument shape.

/// The argument shape of an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    /// No arguments.
    None,
    /// Exactly one of these values.
    OneOf(&'static [&'static str]),
    /// Zero or one of these values.
    Optional(&'static [&'static str]),
    /// A free-form argument (folder name, path, …).
    Text,
    /// One of these values plus a free-form name (`item.copy field <name>`).
    OneOfThenText(&'static [&'static str]),
    /// A number.
    Number,
}

/// One action.
#[derive(Debug, Clone, Copy)]
pub struct ActionDef {
    /// Stable id.
    pub id: &'static str,
    /// UI description.
    pub description: &'static str,
    /// Contexts where the action can be bound; empty means every context.
    pub contexts: &'static [&'static str],
    /// Whether a count prefix changes the action.
    pub accepts_count: bool,
    /// Argument shape.
    pub args: Args,
}

const ALL: &[&str] = &[];

macro_rules! action {
    ($id:literal, $desc:literal, $ctx:expr, $count:expr, $args:expr) => {
        ActionDef {
            id: $id,
            description: $desc,
            contexts: $ctx,
            accepts_count: $count,
            args: $args,
        }
    };
}

/// Every action, in registry order.
pub static ACTIONS: &[ActionDef] = &[
    action!("app.quit", "Quit lazybw", &["global", "unlock"], false, Args::None),
    action!("app.quit_force", "Quit and discard unsaved changes", &["global"], false, Args::None),
    action!("app.cancel", "Cancel", ALL, false, Args::None),
    action!("app.help", "Show help for this screen", &["global"], false, Args::None),
    action!("app.redraw", "Redraw the whole screen", &["global"], false, Args::None),
    action!("app.messages", "Show message history", &["global"], false, Args::None),
    action!("app.settings", "Open settings", &["global"], false, Args::None),
    action!("app.keymap", "Edit keybindings", &["global"], false, Args::None),
    action!("app.doctor", "Show diagnostics", &["global"], false, Args::None),
    action!("cmdline.open", "Open the command line", &["global", "notes"], false, Args::None),
    action!("quick_open", "Find any item", &["global"], false, Args::None),
    action!("focus.left", "Focus the pane on the left", &["global"], false, Args::None),
    action!("focus.right", "Focus the pane on the right", &["global"], false, Args::None),
    action!("focus.next", "Focus the next pane", &["global"], false, Args::None),
    action!("focus.prev", "Focus the previous pane", &["global"], false, Args::None),
    action!("layout.toggle_sidebar", "Show or hide the sidebar", &["global"], false, Args::None),
    action!(
        "list.toggle_density",
        "Switch between compact and comfortable rows",
        &["global"],
        false,
        Args::None
    ),
    action!("tab.next", "Next tab", &["global"], false, Args::None),
    action!("tab.prev", "Previous tab", &["global"], false, Args::None),
    action!("tab.goto", "Go to tab", &["global"], false, Args::Number),
    action!("jump.back", "Jump back", &["global"], false, Args::None),
    action!("jump.forward", "Jump forward", &["global"], false, Args::None),
    action!("theme.pick", "Choose a theme", &["global"], false, Args::None),
    action!("theme.set", "Use theme", &["global"], false, Args::Text),
    // Movement
    action!("nav.down", "Move down", ALL, true, Args::None),
    action!("nav.up", "Move up", ALL, true, Args::None),
    action!("nav.top", "Go to the first row", ALL, true, Args::None),
    action!("nav.bottom", "Go to the last row", ALL, true, Args::None),
    action!("nav.half_down", "Half a page down", ALL, false, Args::None),
    action!("nav.half_up", "Half a page up", ALL, false, Args::None),
    action!("nav.page_down", "A page down", ALL, false, Args::None),
    action!("nav.page_up", "A page up", ALL, false, Args::None),
    action!("nav.screen_top", "Top of the screen", ALL, false, Args::None),
    action!("nav.screen_middle", "Middle of the screen", ALL, false, Args::None),
    action!("nav.screen_bottom", "Bottom of the screen", ALL, false, Args::None),
    action!("view.center", "Scroll so the cursor is centred", ALL, false, Args::None),
    action!("view.top", "Scroll so the cursor is at the top", ALL, false, Args::None),
    action!("view.bottom", "Scroll so the cursor is at the bottom", ALL, false, Args::None),
    action!("search.start", "Search in this list", ALL, false, Args::None),
    action!("search.next", "Next match", ALL, false, Args::None),
    action!("search.prev", "Previous match", ALL, false, Args::None),
    action!("search.clear", "Clear the search", ALL, false, Args::None),
    // Vault
    action!("vault.sync", "Sync with the server", &["global"], false, Args::Optional(&["force"])),
    action!("vault.lock", "Lock the vault", &["global"], false, Args::None),
    action!("vault.logout", "Log out", &["global"], false, Args::None),
    action!("vault.export", "Export the vault", &["global"], false, Args::None),
    action!("vault.import", "Import into the vault", &["global"], false, Args::None),
    action!("generator.open", "Open the generator", &["global"], false, Args::None),
    action!("health.open", "Open vault health", &["global"], false, Args::None),
    action!(
        "sort.set",
        "Sort by …",
        &["global"],
        false,
        Args::OneOf(&["name", "modified", "created", "type"])
    ),
    action!("sort.reverse", "Reverse the sort order", &["global"], false, Args::None),
    action!("filter.apply", "Show these items", &["sidebar"], false, Args::None),
    action!("fold.toggle", "Toggle folder", &["sidebar"], false, Args::None),
    action!(
        "fold.expand_or_apply",
        "Open folder, or show its items",
        &["sidebar"],
        false,
        Args::None
    ),
    action!(
        "fold.collapse_or_parent",
        "Close folder, or go to its parent",
        &["sidebar"],
        false,
        Args::None
    ),
    action!("fold.open", "Open folder", &["sidebar"], false, Args::None),
    action!("fold.close", "Close folder", &["sidebar"], false, Args::None),
    action!("fold.open_all", "Open all folders", &["sidebar"], false, Args::None),
    action!("fold.close_all", "Close all folders", &["sidebar"], false, Args::None),
    action!("folder.new", "New folder", &["sidebar", "global"], false, Args::Optional(&[])),
    action!("folder.rename", "Rename folder", &["sidebar", "global"], false, Args::Optional(&[])),
    action!("folder.delete", "Delete folder", &["sidebar", "global"], false, Args::Optional(&[])),
    action!("folder.jump", "Go to folder", &["global"], false, Args::Optional(&[])),
    action!("trash.empty", "Empty the trash", &["list"], false, Args::None),
    // Items
    action!("item.open", "Open item", &["list"], false, Args::None),
    action!(
        "item.copy",
        "Copy …",
        &["list", "detail", "visual"],
        false,
        Args::OneOfThenText(&[
            "primary",
            "username",
            "password",
            "totp",
            "uri",
            "notes",
            "number",
            "code",
            "public_key",
            "private_key",
            "field",
        ])
    ),
    action!("item.copy_field", "Copy this field", &["detail"], false, Args::None),
    action!(
        "item.reveal_toggle",
        "Show or hide this field",
        &["list", "detail", "editor"],
        false,
        Args::None
    ),
    action!("item.reveal_all", "Show all fields", &["list", "detail"], false, Args::None),
    action!("item.hide_all", "Hide all fields", &["list", "detail"], false, Args::None),
    action!(
        "item.new",
        "New item",
        &["list", "global"],
        false,
        Args::Optional(&["login", "note", "card", "identity", "ssh"])
    ),
    action!("item.edit", "Edit item", &["list", "detail", "health"], false, Args::None),
    action!("item.edit_field", "Edit this field", &["detail"], false, Args::None),
    action!("item.change_field", "Replace this field", &["detail"], false, Args::None),
    action!(
        "item.edit_json",
        "Edit as JSON in $EDITOR",
        &["list", "detail", "editor"],
        false,
        Args::None
    ),
    action!("item.trash", "Move to trash", &["list", "visual"], true, Args::None),
    action!("item.delete", "Delete permanently", &["list", "visual"], false, Args::None),
    action!("item.restore", "Restore from trash", &["list", "visual"], false, Args::None),
    action!(
        "item.favorite",
        "Add to or remove from favorites",
        &["list", "detail", "visual"],
        false,
        Args::None
    ),
    action!(
        "item.move",
        "Move to folder",
        &["list", "detail", "visual"],
        false,
        Args::Optional(&[])
    ),
    action!("item.yank", "Copy item for cloning", &["list"], false, Args::None),
    action!("item.paste_clone", "Paste a clone here", &["list"], false, Args::None),
    action!("item.open_uri", "Open website", &["list", "detail"], false, Args::Optional(&[])),
    action!("item.history", "Password history", &["list", "detail"], false, Args::None),
    action!("item.info", "Item details", &["list", "detail"], false, Args::None),
    action!("item.remove_passkey", "Remove this passkey", &["detail"], false, Args::None),
    action!(
        "item.generate_password",
        "Generate and save a new password",
        &["list", "detail", "health"],
        false,
        Args::None
    ),
    action!("item.goto_folder", "Go to this item's folder", &["list", "detail"], false, Args::None),
    action!("undo", "Undo", &["list", "detail", "visual", "sidebar"], false, Args::None),
    action!("redo", "Redo", &["list", "detail", "visual", "sidebar"], false, Args::None),
    action!("mark.toggle", "Mark or unmark", &["list", "visual"], false, Args::None),
    action!("visual.start", "Select a range", &["list"], false, Args::None),
    action!("visual.exit", "Clear the selection", &["visual"], false, Args::None),
    // Editor
    action!("editor.save", "Save", &["editor", "insert", "notes"], false, Args::None),
    action!("editor.save_close", "Save and close", &["editor"], false, Args::None),
    action!("editor.close", "Close", &["editor"], false, Args::None),
    action!("editor.column_left", "Left column", &["editor"], false, Args::None),
    action!("editor.column_right", "Right column", &["editor"], false, Args::None),
    action!("editor.discard", "Close without saving", &["editor"], false, Args::None),
    action!("editor.reload", "Reload from the server", &["editor"], false, Args::None),
    action!("editor.next_field", "Next field", &["editor", "insert", "notes"], false, Args::None),
    action!(
        "editor.prev_field",
        "Previous field",
        &["editor", "insert", "notes"],
        false,
        Args::None
    ),
    action!("editor.first_field", "First field", &["editor"], false, Args::None),
    action!("editor.last_field", "Last field", &["editor"], false, Args::None),
    action!("editor.insert", "Insert", &["editor"], false, Args::None),
    action!("editor.append", "Append", &["editor"], false, Args::None),
    action!("editor.insert_start", "Insert at start", &["editor"], false, Args::None),
    action!("editor.append_end", "Append at end", &["editor"], false, Args::None),
    action!("editor.change_field", "Replace the value", &["editor"], false, Args::None),
    action!("editor.normal", "Back to NORMAL", &["insert"], false, Args::None),
    action!("editor.row_add_below", "New row below", &["editor"], false, Args::None),
    action!("editor.row_add_above", "New row above", &["editor"], false, Args::None),
    action!("editor.row_delete", "Delete row or clear field", &["editor"], false, Args::None),
    action!("editor.cycle_next", "Next option", &["editor", "insert"], false, Args::None),
    action!("editor.cycle_prev", "Previous option", &["editor", "insert"], false, Args::None),
    action!("editor.activate", "Open picker or toggle", &["editor"], false, Args::None),
    action!("editor.generate", "Generate password", &["editor", "insert"], false, Args::None),
    action!(
        "editor.external",
        "Open in $EDITOR",
        &["editor", "insert", "notes"],
        false,
        Args::None
    ),
    action!("editor.undo", "Undo change", &["editor"], false, Args::None),
    action!("editor.redo", "Redo change", &["editor"], false, Args::None),
    // Tools
    action!("generator.regenerate", "Generate another", &["generator"], false, Args::None),
    action!("generator.copy", "Copy", &["generator"], false, Args::None),
    action!("generator.toggle_mode", "Password / passphrase", &["generator"], false, Args::None),
    action!("generator.increase", "Increase value", &["generator"], false, Args::None),
    action!("generator.decrease", "Decrease value", &["generator"], false, Args::None),
    action!("generator.toggle_option", "Turn option on or off", &["generator"], false, Args::None),
    action!("generator.use", "Use this password", &["generator"], false, Args::None),
    action!("health.rerun", "Run the check again", &["health"], false, Args::None),
    action!("health.next_category", "Next category", &["health"], false, Args::None),
    action!("health.prev_category", "Previous category", &["health"], false, Args::None),
    action!("health.open_item", "Open item", &["health"], false, Args::None),
    action!("quick_open.next", "Next result", &["quick_open"], false, Args::None),
    action!("quick_open.prev", "Previous result", &["quick_open"], false, Args::None),
    action!("quick_open.accept", "Open", &["quick_open"], false, Args::None),
    action!("quick_open.copy_password", "Copy password", &["quick_open"], false, Args::None),
    action!("quick_open.copy_username", "Copy username", &["quick_open"], false, Args::None),
    action!("quick_open.copy_totp", "Copy code", &["quick_open"], false, Args::None),
    action!("quick_open.edit", "Edit", &["quick_open"], false, Args::None),
    action!("cmdline.execute", "Run", &["cmdline"], false, Args::None),
    action!("cmdline.complete_next", "Next completion", &["cmdline"], false, Args::None),
    action!("cmdline.complete_prev", "Previous completion", &["cmdline"], false, Args::None),
    action!("cmdline.history_prev", "Previous command", &["cmdline"], false, Args::None),
    action!("cmdline.history_next", "Next command", &["cmdline"], false, Args::None),
    action!("picker.next", "Next", &["picker"], false, Args::None),
    action!("picker.prev", "Previous", &["picker"], false, Args::None),
    action!("picker.accept", "Choose", &["picker"], false, Args::None),
    action!("confirm.yes", "Confirm", &["confirm"], false, Args::None),
    action!("confirm.no", "Cancel", &["confirm"], false, Args::None),
    action!("unlock.submit", "Unlock", &["unlock"], false, Args::None),
    action!("unlock.logout", "Log out", &["unlock"], false, Args::None),
    action!("settings.activate", "Change setting", &["settings"], false, Args::None),
    action!("settings.next_value", "Next value", &["settings"], false, Args::None),
    action!("settings.prev_value", "Previous value", &["settings"], false, Args::None),
    action!("settings.reset", "Reset setting", &["settings"], false, Args::None),
    action!("keymap.rebind", "Rebind", &["keymap_editor"], false, Args::None),
    action!("keymap.add", "Add key", &["keymap_editor"], false, Args::None),
    action!("keymap.remove", "Remove key", &["keymap_editor"], false, Args::None),
    action!("keymap.reset", "Reset action", &["keymap_editor"], false, Args::None),
    action!("keymap.reset_all", "Reset all", &["keymap_editor"], false, Args::None),
    action!("keymap.save", "Save", &["keymap_editor"], false, Args::None),
    action!("keymap.next_context", "Next context", &["keymap_editor"], false, Args::None),
    action!("keymap.prev_context", "Previous context", &["keymap_editor"], false, Args::None),
    action!("keymap.filter", "Filter", &["keymap_editor"], false, Args::None),
    action!("help.search", "Search help", &["help"], false, Args::None),
    action!("login.next", "Next field", &["login"], false, Args::None),
    action!("login.prev", "Previous field", &["login"], false, Args::None),
    action!("login.submit", "Continue", &["login"], false, Args::None),
    action!("login.back", "Back", &["login"], false, Args::None),
    action!("noop", "Do nothing (used to unbind)", ALL, false, Args::None),
];

/// Looks up an action definition.
#[must_use]
pub fn get(id: &str) -> Option<&'static ActionDef> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// True when the id is in the registry.
#[must_use]
pub fn is_known(id: &str) -> bool {
    get(id).is_some()
}

/// The description shown in which-key, help and the keymap editor.
#[must_use]
pub fn description(id: &str) -> &'static str {
    get(id).map_or("Unknown action", |a| a.description)
}

/// True when the action can be bound in a context.
#[must_use]
pub fn available_in(id: &str, context: &str) -> bool {
    let Some(def) = get(id) else { return false };
    def.contexts.is_empty() || def.contexts.contains(&context)
}

/// True when the action uses a count prefix.
#[must_use]
pub fn accepts_count(id: &str) -> bool {
    get(id).is_some_and(|a| a.accepts_count)
}

/// Validates the arguments of an action call; the message is shown verbatim.
#[must_use]
pub fn check_args(id: &str, args: &[String]) -> Option<String> {
    let def = get(id)?;
    match def.args {
        Args::None => {
            if args.is_empty() {
                None
            } else {
                Some(format!("{id} takes no arguments"))
            }
        }
        Args::OneOf(values) => {
            if args.len() == 1 && values.contains(&args[0].as_str()) {
                None
            } else {
                Some(format!("{id} expects one of {}", values.join(", ")))
            }
        }
        Args::Optional(values) => {
            if args.is_empty()
                || (args.len() == 1 && (values.is_empty() || values.contains(&args[0].as_str())))
            {
                None
            } else {
                Some(format!("{id} accepts {args:?}"))
            }
        }
        Args::Text => {
            if args.len() == 1 {
                None
            } else {
                Some(format!("{id} expects one argument"))
            }
        }
        Args::OneOfThenText(values) => {
            let simple = args.len() == 1 && values.contains(&args[0].as_str());
            let field = args.len() == 2 && args[0] == "field" && values.contains(&args[0].as_str());
            if simple || field {
                None
            } else {
                Some(format!("{id} expects one of {}", values.join(", ")))
            }
        }
        Args::Number => {
            if args.len() == 1 && args[0].parse::<u32>().is_ok() {
                None
            } else {
                Some(format!("{id} expects a number"))
            }
        }
    }
}

/// Suggests the closest known action id, for typo messages.
#[must_use]
pub fn suggest(input: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for def in ACTIONS {
        let distance = levenshtein(input, def.id);
        if distance <= 2 && best.is_none_or(|(_, d)| distance < d) {
            best = Some((def.id, distance));
        }
    }
    best.map(|(id, _)| id)
}

/// Actions grouped by area, for the help screen.
#[must_use]
pub fn groups() -> Vec<(&'static str, Vec<&'static ActionDef>)> {
    let mut groups: Vec<(&'static str, Vec<&'static ActionDef>)> = Vec::new();
    for def in ACTIONS {
        let area = def.id.split('.').next().unwrap_or("other");
        let label = match area {
            "app" => "Application",
            "cmdline" => "Command line",
            "focus" | "layout" | "tab" => "Layout",
            "theme" => "Appearance",
            "nav" | "view" => "Movement",
            "search" => "Search",
            "jump" => "Jump list",
            "vault" => "Vault",
            "sort" | "filter" | "fold" => "Filtering",
            "folder" => "Folders",
            "trash" => "Trash",
            "item" => "Items",
            "undo" | "redo" => "Undo",
            "mark" | "visual" => "Selection",
            "editor" => "Editor",
            "generator" => "Generator",
            "health" => "Health",
            "quick_open" => "Quick open",
            "picker" | "confirm" | "unlock" => "Dialogs",
            "settings" => "Settings",
            "keymap" => "Keybindings",
            "help" => "Help",
            "login" => "Login",
            other => other,
        };
        match groups.iter_mut().find(|(name, _)| *name == label) {
            Some((_, list)) => list.push(def),
            None => groups.push((label, vec![def])),
        }
    }
    groups
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (prev[j + 1] + 1).min(current[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut current);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        let mut seen = HashSet::new();
        for def in ACTIONS {
            assert!(seen.insert(def.id), "duplicate action id {}", def.id);
        }
    }

    #[test]
    fn ids_follow_domain_verb() {
        for def in ACTIONS {
            assert!(
                def.id.contains('.') || matches!(def.id, "quick_open" | "undo" | "redo" | "noop"),
                "{} has no dot",
                def.id
            );
            assert!(
                def.id.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "{} is not snake case",
                def.id
            );
            assert!(!def.description.is_empty());
        }
    }

    #[test]
    fn context_checks() {
        assert!(available_in("item.trash", "list"));
        assert!(!available_in("item.trash", "sidebar"));
        assert!(available_in("app.cancel", "picker"));
        assert!(available_in("nav.down", "anything"));
        assert!(!available_in("nope", "list"));
    }

    #[test]
    fn arguments_are_validated() {
        assert!(check_args("item.copy", &["password".into()]).is_none());
        assert!(check_args("item.copy", &["bogus".into()]).is_some());
        assert!(check_args("item.copy", &["field".into(), "Recovery".into()]).is_none());
        assert!(check_args("item.new", &[]).is_none());
        assert!(check_args("item.new", &["login".into()]).is_none());
        assert!(check_args("item.new", &["bogus".into()]).is_some());
        assert!(check_args("tab.goto", &["2".into()]).is_none());
        assert!(check_args("tab.goto", &["x".into()]).is_some());
        assert!(check_args("app.quit", &["x".into()]).is_some());
    }

    #[test]
    fn typo_suggestions() {
        assert_eq!(suggest("item.cpy"), Some("item.copy"));
        assert_eq!(suggest("nav.dow"), Some("nav.down"));
    }

    #[test]
    fn groups_cover_every_action() {
        let total: usize = groups().iter().map(|(_, v)| v.len()).sum();
        assert_eq!(total, ACTIONS.len());
    }
}
