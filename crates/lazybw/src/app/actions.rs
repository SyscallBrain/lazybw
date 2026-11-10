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
    /// The typed id the dispatcher matches on.
    pub action: ActionId,
    /// Stable id, as written in `config.toml`, `:map` and the help screen.
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

/// Builds [`ActionId`] and [`ACTIONS`] from one list, so an id the
/// dispatcher matches on always has a registry entry and the other way round.
macro_rules! registry {
    ($( $(#[$meta:meta])* $variant:ident = $id:literal, $desc:literal, $ctx:expr, $count:expr, $args:expr; )*) => {
        /// Every user-invokable action. Keys, `:` commands, pickers and menus
        /// resolve to one of these; the string form lives only in the config.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub enum ActionId {
            $( $(#[$meta])* #[doc = $desc] $variant, )*
        }

        /// Every action, in registry order (the order of [`ActionId`]).
        pub static ACTIONS: &[ActionDef] = &[
            $( ActionDef {
                action: ActionId::$variant,
                id: $id,
                description: $desc,
                contexts: $ctx,
                accepts_count: $count,
                args: $args,
            }, )*
        ];
    };
}

impl ActionId {
    /// The registry entry.
    #[must_use]
    pub fn def(self) -> &'static ActionDef {
        &ACTIONS[self as usize]
    }

    /// The stable string id (`item.copy`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.def().id
    }

    /// Parses a stable string id.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        get(id).map(|def| def.action)
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

registry! {
    AppQuit = "app.quit", "Quit lazybw", &["global", "unlock"], false, Args::None;
    AppQuitForce = "app.quit_force", "Quit and discard unsaved changes", &["global"], false, Args::None;
    AppCancel = "app.cancel", "Cancel", ALL, false, Args::None;
    AppHelp = "app.help", "Show help for this screen", &["global"], false, Args::None;
    AppRedraw = "app.redraw", "Redraw the whole screen", &["global"], false, Args::None;
    AppMessages = "app.messages", "Show message history", &["global"], false, Args::None;
    AppSettings = "app.settings", "Open settings", &["global"], false, Args::None;
    AppKeymap = "app.keymap", "Edit keybindings", &["global"], false, Args::None;
    AppDoctor = "app.doctor", "Show diagnostics", &["global"], false, Args::None;
    CmdlineOpen = "cmdline.open", "Open the command line", &["global", "notes"], false, Args::None;
    QuickOpen = "quick_open", "Find any item", &["global"], false, Args::None;
    FocusLeft = "focus.left", "Focus the pane on the left", &["global"], false, Args::None;
    FocusRight = "focus.right", "Focus the pane on the right", &["global"], false, Args::None;
    FocusNext = "focus.next", "Focus the next pane", &["global"], false, Args::None;
    FocusPrev = "focus.prev", "Focus the previous pane", &["global"], false, Args::None;
    LayoutToggleSidebar = "layout.toggle_sidebar", "Show or hide the sidebar", &["global"], false, Args::None;
    ListToggleDensity = "list.toggle_density", "Switch between compact and comfortable rows", &["global"], false, Args::None;
    TabNext = "tab.next", "Next tab", &["global"], false, Args::None;
    TabPrev = "tab.prev", "Previous tab", &["global"], false, Args::None;
    TabGoto = "tab.goto", "Go to tab", &["global"], false, Args::Number;
    JumpBack = "jump.back", "Jump back", &["global"], false, Args::None;
    JumpForward = "jump.forward", "Jump forward", &["global"], false, Args::None;
    ThemePick = "theme.pick", "Choose a theme", &["global"], false, Args::None;
    ThemeSet = "theme.set", "Use theme", &["global"], false, Args::Text;
    // Movement
    NavDown = "nav.down", "Move down", ALL, true, Args::None;
    NavUp = "nav.up", "Move up", ALL, true, Args::None;
    NavTop = "nav.top", "Go to the first row", ALL, true, Args::None;
    NavBottom = "nav.bottom", "Go to the last row", ALL, true, Args::None;
    NavHalfDown = "nav.half_down", "Half a page down", ALL, false, Args::None;
    NavHalfUp = "nav.half_up", "Half a page up", ALL, false, Args::None;
    NavPageDown = "nav.page_down", "A page down", ALL, false, Args::None;
    NavPageUp = "nav.page_up", "A page up", ALL, false, Args::None;
    NavScreenTop = "nav.screen_top", "Top of the screen", ALL, false, Args::None;
    NavScreenMiddle = "nav.screen_middle", "Middle of the screen", ALL, false, Args::None;
    NavScreenBottom = "nav.screen_bottom", "Bottom of the screen", ALL, false, Args::None;
    ViewCenter = "view.center", "Scroll so the cursor is centred", ALL, false, Args::None;
    ViewTop = "view.top", "Scroll so the cursor is at the top", ALL, false, Args::None;
    ViewBottom = "view.bottom", "Scroll so the cursor is at the bottom", ALL, false, Args::None;
    SearchStart = "search.start", "Search in this list", ALL, false, Args::None;
    SearchNext = "search.next", "Next match", ALL, false, Args::None;
    SearchPrev = "search.prev", "Previous match", ALL, false, Args::None;
    SearchClear = "search.clear", "Clear the search", ALL, false, Args::None;
    // Vault
    VaultSync = "vault.sync", "Sync with the server", &["global"], false, Args::Optional(&["force"]);
    VaultLock = "vault.lock", "Lock the vault", &["global"], false, Args::None;
    VaultLogout = "vault.logout", "Log out", &["global"], false, Args::None;
    VaultExport = "vault.export", "Export the vault", &["global"], false, Args::None;
    VaultImport = "vault.import", "Import into the vault", &["global"], false, Args::None;
    GeneratorOpen = "generator.open", "Open the generator", &["global"], false, Args::None;
    HealthOpen = "health.open", "Open vault health", &["global"], false, Args::None;
    SortSet = "sort.set", "Sort by …", &["global"], false, Args::OneOf(&["name", "modified", "created", "type"]);
    SortReverse = "sort.reverse", "Reverse the sort order", &["global"], false, Args::None;
    FilterApply = "filter.apply", "Show these items", &["sidebar"], false, Args::None;
    FoldToggle = "fold.toggle", "Toggle folder", &["sidebar"], false, Args::None;
    FoldExpandOrApply = "fold.expand_or_apply", "Open folder, or show its items", &["sidebar"], false, Args::None;
    FoldCollapseOrParent = "fold.collapse_or_parent", "Close folder, or go to its parent", &["sidebar"], false, Args::None;
    FoldOpen = "fold.open", "Open folder", &["sidebar"], false, Args::None;
    FoldClose = "fold.close", "Close folder", &["sidebar"], false, Args::None;
    FoldOpenAll = "fold.open_all", "Open all folders", &["sidebar"], false, Args::None;
    FoldCloseAll = "fold.close_all", "Close all folders", &["sidebar"], false, Args::None;
    FolderNew = "folder.new", "New folder", &["sidebar", "global"], false, Args::Optional(&[]);
    FolderRename = "folder.rename", "Rename folder", &["sidebar", "global"], false, Args::Optional(&[]);
    FolderDelete = "folder.delete", "Delete folder", &["sidebar", "global"], false, Args::Optional(&[]);
    FolderJump = "folder.jump", "Go to folder", &["global"], false, Args::Optional(&[]);
    TrashEmpty = "trash.empty", "Empty the trash", &["list"], false, Args::None;
    // Items
    ItemOpen = "item.open", "Open item", &["list"], false, Args::None;
    ItemCopy = "item.copy", "Copy …", &["list", "detail", "visual"], false, Args::OneOfThenText(&[ "primary", "username", "password", "totp", "uri", "notes", "number", "code", "public_key", "private_key", "field", ]);
    ItemCopyField = "item.copy_field", "Copy this field", &["detail"], false, Args::None;
    ItemRevealToggle = "item.reveal_toggle", "Show or hide this field", &["list", "detail", "editor"], false, Args::None;
    ItemRevealAll = "item.reveal_all", "Show all fields", &["list", "detail"], false, Args::None;
    ItemHideAll = "item.hide_all", "Hide all fields", &["list", "detail"], false, Args::None;
    ItemNew = "item.new", "New item", &["list", "global"], false, Args::Optional(&["login", "note", "card", "identity", "ssh"]);
    ItemEdit = "item.edit", "Edit item", &["list", "detail", "health"], false, Args::None;
    ItemEditField = "item.edit_field", "Edit this field", &["detail"], false, Args::None;
    ItemChangeField = "item.change_field", "Replace this field", &["detail"], false, Args::None;
    ItemEditJson = "item.edit_json", "Edit as JSON in $EDITOR", &["list", "detail", "editor"], false, Args::None;
    ItemTrash = "item.trash", "Move to trash", &["list", "visual"], true, Args::None;
    ItemDelete = "item.delete", "Delete permanently", &["list", "visual"], false, Args::None;
    ItemRestore = "item.restore", "Restore from trash", &["list", "visual"], false, Args::None;
    ItemFavorite = "item.favorite", "Add to or remove from favorites", &["list", "detail", "visual"], false, Args::None;
    ItemMove = "item.move", "Move to folder", &["list", "detail", "visual"], false, Args::Optional(&[]);
    ItemYank = "item.yank", "Copy item for cloning", &["list"], false, Args::None;
    ItemPasteClone = "item.paste_clone", "Paste a clone here", &["list"], false, Args::None;
    ItemOpenUri = "item.open_uri", "Open website", &["list", "detail"], false, Args::Optional(&[]);
    ItemHistory = "item.history", "Password history", &["list", "detail"], false, Args::None;
    ItemInfo = "item.info", "Item details", &["list", "detail"], false, Args::None;
    ItemRemovePasskey = "item.remove_passkey", "Remove this passkey", &["detail"], false, Args::None;
    ItemGeneratePassword = "item.generate_password", "Generate and save a new password", &["list", "detail", "health"], false, Args::None;
    ItemGotoFolder = "item.goto_folder", "Go to this item's folder", &["list", "detail"], false, Args::None;
    Undo = "undo", "Undo", &["list", "detail", "visual", "sidebar"], false, Args::None;
    Redo = "redo", "Redo", &["list", "detail", "visual", "sidebar"], false, Args::None;
    MarkToggle = "mark.toggle", "Mark or unmark", &["list", "visual"], false, Args::None;
    VisualStart = "visual.start", "Select a range", &["list"], false, Args::None;
    VisualExit = "visual.exit", "Clear the selection", &["visual"], false, Args::None;
    // Editor
    EditorSave = "editor.save", "Save", &["editor", "insert", "notes"], false, Args::None;
    EditorSaveClose = "editor.save_close", "Save and close", &["editor"], false, Args::None;
    EditorClose = "editor.close", "Close", &["editor"], false, Args::None;
    EditorColumnLeft = "editor.column_left", "Left column", &["editor"], false, Args::None;
    EditorColumnRight = "editor.column_right", "Right column", &["editor"], false, Args::None;
    EditorDiscard = "editor.discard", "Close without saving", &["editor"], false, Args::None;
    EditorReload = "editor.reload", "Reload from the server", &["editor"], false, Args::None;
    EditorNextField = "editor.next_field", "Next field", &["editor", "insert", "notes"], false, Args::None;
    EditorPrevField = "editor.prev_field", "Previous field", &["editor", "insert", "notes"], false, Args::None;
    EditorFirstField = "editor.first_field", "First field", &["editor"], false, Args::None;
    EditorLastField = "editor.last_field", "Last field", &["editor"], false, Args::None;
    EditorInsert = "editor.insert", "Insert", &["editor"], false, Args::None;
    EditorAppend = "editor.append", "Append", &["editor"], false, Args::None;
    EditorInsertStart = "editor.insert_start", "Insert at start", &["editor"], false, Args::None;
    EditorAppendEnd = "editor.append_end", "Append at end", &["editor"], false, Args::None;
    EditorChangeField = "editor.change_field", "Replace the value", &["editor"], false, Args::None;
    EditorNormal = "editor.normal", "Back to NORMAL", &["insert"], false, Args::None;
    EditorRowAddBelow = "editor.row_add_below", "New row below", &["editor"], false, Args::None;
    EditorRowAddAbove = "editor.row_add_above", "New row above", &["editor"], false, Args::None;
    EditorRowDelete = "editor.row_delete", "Delete row or clear field", &["editor"], false, Args::None;
    EditorCycleNext = "editor.cycle_next", "Next option", &["editor", "insert"], false, Args::None;
    EditorCyclePrev = "editor.cycle_prev", "Previous option", &["editor", "insert"], false, Args::None;
    EditorActivate = "editor.activate", "Open picker or toggle", &["editor"], false, Args::None;
    EditorGenerate = "editor.generate", "Generate password", &["editor", "insert"], false, Args::None;
    EditorExternal = "editor.external", "Open in $EDITOR", &["editor", "insert", "notes"], false, Args::None;
    EditorUndo = "editor.undo", "Undo change", &["editor"], false, Args::None;
    EditorRedo = "editor.redo", "Redo change", &["editor"], false, Args::None;
    // Tools
    GeneratorRegenerate = "generator.regenerate", "Generate another", &["generator"], false, Args::None;
    GeneratorCopy = "generator.copy", "Copy", &["generator"], false, Args::None;
    GeneratorToggleMode = "generator.toggle_mode", "Password / passphrase", &["generator"], false, Args::None;
    GeneratorIncrease = "generator.increase", "Increase value", &["generator"], false, Args::None;
    GeneratorDecrease = "generator.decrease", "Decrease value", &["generator"], false, Args::None;
    GeneratorToggleOption = "generator.toggle_option", "Turn option on or off", &["generator"], false, Args::None;
    GeneratorUse = "generator.use", "Use this password", &["generator"], false, Args::None;
    HealthRerun = "health.rerun", "Run the check again", &["health"], false, Args::None;
    HealthNextCategory = "health.next_category", "Next category", &["health"], false, Args::None;
    HealthPrevCategory = "health.prev_category", "Previous category", &["health"], false, Args::None;
    HealthOpenItem = "health.open_item", "Open item", &["health"], false, Args::None;
    QuickOpenNext = "quick_open.next", "Next result", &["quick_open"], false, Args::None;
    QuickOpenPrev = "quick_open.prev", "Previous result", &["quick_open"], false, Args::None;
    QuickOpenAccept = "quick_open.accept", "Open", &["quick_open"], false, Args::None;
    QuickOpenCopyPassword = "quick_open.copy_password", "Copy password", &["quick_open"], false, Args::None;
    QuickOpenCopyUsername = "quick_open.copy_username", "Copy username", &["quick_open"], false, Args::None;
    QuickOpenCopyTotp = "quick_open.copy_totp", "Copy code", &["quick_open"], false, Args::None;
    QuickOpenEdit = "quick_open.edit", "Edit", &["quick_open"], false, Args::None;
    CmdlineExecute = "cmdline.execute", "Run", &["cmdline"], false, Args::None;
    CmdlineCompleteNext = "cmdline.complete_next", "Next completion", &["cmdline"], false, Args::None;
    CmdlineCompletePrev = "cmdline.complete_prev", "Previous completion", &["cmdline"], false, Args::None;
    CmdlineHistoryPrev = "cmdline.history_prev", "Previous command", &["cmdline"], false, Args::None;
    CmdlineHistoryNext = "cmdline.history_next", "Next command", &["cmdline"], false, Args::None;
    PickerNext = "picker.next", "Next", &["picker"], false, Args::None;
    PickerPrev = "picker.prev", "Previous", &["picker"], false, Args::None;
    PickerAccept = "picker.accept", "Choose", &["picker"], false, Args::None;
    ConfirmYes = "confirm.yes", "Confirm", &["confirm"], false, Args::None;
    ConfirmNo = "confirm.no", "Cancel", &["confirm"], false, Args::None;
    UnlockSubmit = "unlock.submit", "Unlock", &["unlock"], false, Args::None;
    UnlockLogout = "unlock.logout", "Log out", &["unlock"], false, Args::None;
    SettingsActivate = "settings.activate", "Change setting", &["settings"], false, Args::None;
    SettingsNextValue = "settings.next_value", "Next value", &["settings"], false, Args::None;
    SettingsPrevValue = "settings.prev_value", "Previous value", &["settings"], false, Args::None;
    SettingsReset = "settings.reset", "Reset setting", &["settings"], false, Args::None;
    KeymapRebind = "keymap.rebind", "Rebind", &["keymap_editor"], false, Args::None;
    KeymapAdd = "keymap.add", "Add key", &["keymap_editor"], false, Args::None;
    KeymapRemove = "keymap.remove", "Remove key", &["keymap_editor"], false, Args::None;
    KeymapReset = "keymap.reset", "Reset action", &["keymap_editor"], false, Args::None;
    KeymapResetAll = "keymap.reset_all", "Reset all", &["keymap_editor"], false, Args::None;
    KeymapSave = "keymap.save", "Save", &["keymap_editor"], false, Args::None;
    KeymapNextContext = "keymap.next_context", "Next context", &["keymap_editor"], false, Args::None;
    KeymapPrevContext = "keymap.prev_context", "Previous context", &["keymap_editor"], false, Args::None;
    KeymapFilter = "keymap.filter", "Filter", &["keymap_editor"], false, Args::None;
    HelpSearch = "help.search", "Search help", &["help"], false, Args::None;
    LoginNext = "login.next", "Next field", &["login"], false, Args::None;
    LoginPrev = "login.prev", "Previous field", &["login"], false, Args::None;
    LoginSubmit = "login.submit", "Continue", &["login"], false, Args::None;
    LoginBack = "login.back", "Back", &["login"], false, Args::None;
    #[default]
    Noop = "noop", "Do nothing (used to unbind)", ALL, false, Args::None;
}

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
    fn typed_ids_round_trip() {
        for (index, def) in ACTIONS.iter().enumerate() {
            assert_eq!(def.action as usize, index);
            assert_eq!(ActionId::parse(def.id), Some(def.action));
            assert_eq!(def.action.as_str(), def.id);
        }
        assert_eq!(ActionId::parse("item.trash.confirm"), None);
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
