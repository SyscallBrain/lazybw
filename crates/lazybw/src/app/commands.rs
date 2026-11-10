//! The `:` command line: a parser from command text to actions and settings
//! changes, plus completion candidates.

use crate::app::actions::ActionId;
use crate::app::state::{Action, AppState, MessageLevel};
use crate::config::SortOrder;
use crate::vault::cache::Filter;

/// How `:set` was invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetMode {
    /// `:set name=value`
    Set,
    /// `:set name?`
    Show,
    /// `:set name!`
    Toggle,
    /// `:set name&`
    Reset,
}

/// The result of parsing a command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run an action.
    Action(Action),
    /// Show a message.
    Message(MessageLevel, String),
    /// Change a setting.
    Set {
        /// Dotted config key.
        key: String,
        /// The value, for `Set`.
        value: Option<String>,
        /// The mode.
        mode: SetMode,
    },
    /// Add a session mapping.
    Map {
        /// Context, when given.
        context: Option<String>,
        /// Keys, when given.
        keys: Option<String>,
        /// Action text.
        action: Option<String>,
    },
    /// Remove a session mapping.
    Unmap {
        /// Context.
        context: Option<String>,
        /// Keys.
        keys: Option<String>,
    },
    /// Switch the sidebar filter.
    SetFilter(Filter),
    /// Generate a password (or a passphrase) and copy it (`:gen 32`).
    Generate {
        /// A passphrase instead of a password.
        passphrase: bool,
        /// The length, or the word count.
        size: Option<u8>,
    },
    /// Nothing.
    None,
}

/// Parses a command line (without the leading `:`).
#[must_use]
pub fn parse(line: &str) -> Command {
    let line = line.trim();
    if line.is_empty() {
        return Command::None;
    }
    let mut parts = line.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();
    let args: Vec<&str> = rest.split_whitespace().collect();

    if let Ok(number) = name.parse::<u32>() {
        return Command::Action(Action::new(ActionId::NavBottom).counted(Some(number)));
    }

    match name {
        "q" | "quit" => Command::Action(Action::new(ActionId::AppQuit)),
        "q!" => Command::Action(Action::new(ActionId::AppQuitForce)),
        // `:qa` quits lazybw even from the editor (`:q` only closes it).
        "qa" | "qall" => Command::Action(Action::with(ActionId::AppQuit, &["all"])),
        "qa!" | "qall!" => Command::Action(Action::with(ActionId::AppQuitForce, &["all"])),
        "w" | "write" => Command::Action(Action::new(ActionId::EditorSave)),
        "wq" | "x" => Command::Action(Action::new(ActionId::EditorSaveClose)),
        "e!" => Command::Action(Action::new(ActionId::EditorReload)),
        "new" => {
            let mut action = Action::new(ActionId::ItemNew);
            if let Some(kind) = args.first() {
                action.args.push((*kind).to_owned());
            }
            Command::Action(action)
        }
        "edit" => {
            if args.first() == Some(&"json") {
                Command::Action(Action::new(ActionId::ItemEditJson))
            } else {
                Command::Action(Action::new(ActionId::ItemEdit))
            }
        }
        "delete" => Command::Action(Action::new(ActionId::ItemTrash)),
        "delete!" => Command::Action(Action::new(ActionId::ItemDelete)),
        "restore" => Command::Action(Action::new(ActionId::ItemRestore)),
        "fav" => Command::Action(Action::new(ActionId::ItemFavorite)),
        "move" => {
            // The whole rest of the line: folder names have spaces.
            let mut action = Action::new(ActionId::ItemMove);
            if !rest.is_empty() {
                action.args.push(rest.to_owned());
            }
            Command::Action(action)
        }
        "copy" => {
            let mut action = Action::new(ActionId::ItemCopy);
            match args.first() {
                Some(field) => action.args.push((*field).to_owned()),
                None => {
                    return Command::Message(
                        MessageLevel::Error,
                        "E471: Argument required".to_owned(),
                    );
                }
            }
            if args.first() == Some(&"field")
                && let Some(name) = after_first_word(rest)
            {
                action.args.push(name.to_owned());
            }
            Command::Action(action)
        }
        "open" => {
            let mut action = Action::new(ActionId::ItemOpenUri);
            if let Some(number) = args.first() {
                action.args.push((*number).to_owned());
            }
            Command::Action(action)
        }
        "history" => Command::Action(Action::new(ActionId::ItemHistory)),
        // `:gen` opens the generator (ADR-010); `:gen 32` and
        // `:gen phrase 6` make one and copy it without leaving the screen.
        "gen" => match args.as_slice() {
            [] => Command::Action(Action::new(ActionId::GeneratorOpen)),
            ["phrase" | "passphrase"] => Command::Generate { passphrase: true, size: None },
            ["phrase" | "passphrase", n] if n.parse::<u8>().is_ok() => {
                Command::Generate { passphrase: true, size: n.parse().ok() }
            }
            [n] if n.parse::<u8>().is_ok() => {
                Command::Generate { passphrase: false, size: n.parse().ok() }
            }
            _ => Command::Message(
                MessageLevel::Error,
                format!("E474: :gen takes a length, or phrase and a word count: {rest}"),
            ),
        },
        "folder" => match args.first().copied() {
            Some("new") => folder_action(ActionId::FolderNew, after_first_word(rest)),
            Some("rename") => folder_action(ActionId::FolderRename, after_first_word(rest)),
            Some("delete") => folder_action(ActionId::FolderDelete, after_first_word(rest)),
            _ => Command::Message(
                MessageLevel::Error,
                "E492: Not an editor command: folder".to_owned(),
            ),
        },
        "sort" => {
            // `!` reverses, as in Vim's `:sort!`.
            let reverse = rest.ends_with('!');
            let text = rest.trim_end_matches('!').trim();
            match SortOrder::parse(text) {
                Some(order) if reverse => {
                    Command::Action(Action::with(ActionId::SortSet, &[order.label(), "reverse"]))
                }
                Some(order) => Command::Action(Action::with(ActionId::SortSet, &[order.label()])),
                None => Command::Message(
                    MessageLevel::Error,
                    format!("E492: Not an editor command: sort {rest}"),
                ),
            }
        }
        "theme" => {
            let mut action = if rest.is_empty() {
                Action::new(ActionId::ThemePick)
            } else {
                Action::with(ActionId::ThemeSet, &[rest])
            };
            action.args.retain(|a| !a.is_empty());
            Command::Action(action)
        }
        "set" => parse_set(rest),
        "map" => parse_map(rest),
        "unmap" => parse_unmap(rest),
        "keymap" => Command::Action(Action::new(ActionId::AppKeymap)),
        "sync" => {
            let mut action = Action::new(ActionId::VaultSync);
            if rest == "!" {
                action.args.push("force".to_owned());
            }
            Command::Action(action)
        }
        "lock" => Command::Action(Action::new(ActionId::VaultLock)),
        "logout" => Command::Action(Action::new(ActionId::VaultLogout)),
        "export" => {
            let mut action = Action::new(ActionId::VaultExport);
            if let Some(format) = args.first() {
                action.args.push((*format).to_owned());
            }
            if let Some(path) = after_first_word(rest) {
                action.args.push(path.to_owned());
            }
            Command::Action(action)
        }
        "import" => {
            let mut action = Action::new(ActionId::VaultImport);
            if let Some(format) = args.first() {
                action.args.push((*format).to_owned());
            }
            if let Some(path) = after_first_word(rest) {
                action.args.push(path.to_owned());
            }
            Command::Action(action)
        }
        "health" => Command::Action(Action::new(ActionId::HealthOpen)),
        "trash" => Command::SetFilter(Filter::Trash),
        "empty-trash" => Command::Action(Action::new(ActionId::TrashEmpty)),
        "messages" => Command::Action(Action::new(ActionId::AppMessages)),
        "help" => Command::Action(Action::new(ActionId::AppHelp)),
        "doctor" => Command::Action(Action::new(ActionId::AppDoctor)),
        _ => Command::Message(MessageLevel::Error, format!("E492: Not an editor command: {line}")),
    }
}

/// What follows the first word of `text`, when there is anything: a folder
/// name, a field name or a path, spaces included (`:folder new Social media`).
fn after_first_word(text: &str) -> Option<&str> {
    let (_, tail) = text.split_once(char::is_whitespace)?;
    Some(tail.trim()).filter(|tail| !tail.is_empty())
}

fn folder_action(id: ActionId, name: Option<&str>) -> Command {
    let mut action = Action::new(id);
    if let Some(name) = name {
        action.args.push(name.to_owned());
    }
    Command::Action(action)
}

fn parse_set(rest: &str) -> Command {
    if rest.is_empty() {
        return Command::Action(Action::new(ActionId::AppSettings));
    }
    let (mode, body) = match rest.chars().last() {
        Some('?') => (SetMode::Show, &rest[..rest.len() - 1]),
        Some('!') => (SetMode::Toggle, &rest[..rest.len() - 1]),
        Some('&') => (SetMode::Reset, &rest[..rest.len() - 1]),
        _ => (SetMode::Set, rest),
    };
    let (key, value) = match body.split_once('=') {
        Some((key, value)) => (key.trim().to_owned(), Some(value.trim().to_owned())),
        None => (body.trim().to_owned(), None),
    };
    if key.is_empty() {
        return Command::Message(MessageLevel::Error, "E471: Argument required".to_owned());
    }
    Command::Set { key, value, mode }
}

fn parse_map(rest: &str) -> Command {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.is_empty() {
        return Command::Action(Action::new(ActionId::AppKeymap));
    }
    if parts.len() == 1 {
        return Command::Map { context: None, keys: None, action: Some(parts[0].to_owned()) };
    }
    // `:map <ctx> <keys> <action…>` or `:map <keys> <action…>`.
    let known_context = crate::config::keymap::KNOWN_CONTEXTS.contains(&parts[0]);
    let (context, keys, action) = if known_context && parts.len() >= 3 {
        (Some(parts[0].to_owned()), Some(parts[1].to_owned()), Some(parts[2..].join(" ")))
    } else {
        (None, Some(parts[0].to_owned()), Some(parts[1..].join(" ")))
    };
    Command::Map { context, keys, action }
}

fn parse_unmap(rest: &str) -> Command {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let known_context =
        parts.first().is_some_and(|p| crate::config::keymap::KNOWN_CONTEXTS.contains(p));
    if known_context {
        Command::Unmap {
            context: parts.first().map(|s| (*s).to_owned()),
            keys: parts.get(1).map(|s| (*s).to_owned()),
        }
    } else {
        Command::Unmap { context: None, keys: parts.first().map(|s| (*s).to_owned()) }
    }
}

/// The command names offered as completions.
pub const COMMANDS: &[&str] = &[
    "q",
    "quit",
    "qa",
    "q!",
    "w",
    "wq",
    "x",
    "e!",
    "new",
    "edit",
    "delete",
    "delete!",
    "restore",
    "fav",
    "move",
    "copy",
    "open",
    "history",
    "gen",
    "folder",
    "sort",
    "theme",
    "set",
    "map",
    "unmap",
    "keymap",
    "sync",
    "sync!",
    "lock",
    "logout",
    "export",
    "import",
    "health",
    "trash",
    "empty-trash",
    "messages",
    "help",
    "doctor",
];

/// Completion candidates for the current input.
#[must_use]
pub fn completions(input: &str, state: &AppState) -> Vec<String> {
    let input = input.trim_start();
    let (head, tail) = match input.split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail),
        None => (input, ""),
    };
    if tail.is_empty() && !input.contains(' ') {
        return COMMANDS
            .iter()
            .filter(|command| command.starts_with(head))
            .map(|command| (*command).to_owned())
            .collect();
    }
    let tail_lower = tail.to_lowercase();
    let mut out: Vec<String> = match head {
        "theme" => {
            let mut names: Vec<String> =
                crate::theme::builtin::names().into_iter().map(str::to_owned).collect();
            let (user, _) = crate::theme::loader::load_user_themes(&state.paths.themes_dir());
            for theme in user {
                if !names.contains(&theme.name) {
                    names.push(theme.name);
                }
            }
            names
        }
        "sort" => vec!["name".into(), "modified".into(), "created".into(), "type".into()],
        "export" => {
            ["json", "encrypted", "account", "csv"].iter().map(|s| (*s).to_owned()).collect()
        }
        "import" => {
            crate::bw::subprocess::FALLBACK_IMPORT_FORMATS.iter().map(|s| (*s).to_owned()).collect()
        }
        "folder" => vec!["new".into(), "rename".into(), "delete".into()],
        "new" => {
            vec!["login".into(), "note".into(), "card".into(), "identity".into(), "ssh".into()]
        }
        "set" => setting_keys(),
        "move" => state.cache.folders().iter().map(|f| f.name.clone()).collect(),
        _ => Vec::new(),
    };
    out.retain(|candidate| candidate.to_lowercase().starts_with(&tail_lower));
    out.truncate(50);
    out
}

/// Every setting name accepted by `:set`.
#[must_use]
pub fn setting_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for (section, names) in [
        (
            "ui",
            &[
                "theme",
                "transparent",
                "icons",
                "animations",
                "list_density",
                "statusline_separator",
                "show_hints",
                "date_format",
                "mouse",
            ][..],
        ),
        (
            "security",
            &[
                "clipboard_timeout",
                "clear_clipboard_on_exit",
                "auto_lock",
                "reveal_timeout",
                "hide_on_focus_lost",
                "reprompt_grace",
                "totp",
            ][..],
        ),
        (
            "vault",
            &["sync_interval", "sort", "search_notes", "show_empty_types", "confirm_trash"][..],
        ),
        (
            "generator",
            &[
                "type",
                "length",
                "uppercase",
                "lowercase",
                "numbers",
                "special",
                "min_numbers",
                "min_special",
                "avoid_ambiguous",
                "words",
                "separator",
                "capitalize",
                "include_number",
            ][..],
        ),
        ("health", &["check_exposed", "old_password_age"][..]),
        ("editor", &["command"][..]),
    ] {
        for name in names {
            keys.push((*name).to_owned());
            keys.push(format!("{section}.{name}"));
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quit_variants() {
        assert_eq!(parse("q"), Command::Action(Action::new(ActionId::AppQuit)));
        assert_eq!(parse("qa!"), Command::Action(Action::with(ActionId::AppQuitForce, &["all"])));
        assert_eq!(parse("wq"), Command::Action(Action::new(ActionId::EditorSaveClose)));
    }

    #[test]
    fn parses_item_commands() {
        assert_eq!(
            parse("copy password"),
            Command::Action(Action::with(ActionId::ItemCopy, &["password"]))
        );
        assert_eq!(
            parse("copy field Recovery codes"),
            Command::Action(Action {
                id: ActionId::ItemCopy,
                args: vec!["field".into(), "Recovery codes".into()],
                count: None
            })
        );
        assert_eq!(
            parse("new login"),
            Command::Action(Action::with(ActionId::ItemNew, &["login"]))
        );
        assert_eq!(parse("delete!"), Command::Action(Action::new(ActionId::ItemDelete)));
        assert_eq!(
            parse("sort modified!"),
            Command::Action(Action::with(ActionId::SortSet, &["modified", "reverse"]))
        );
        assert_eq!(parse("sort name"), Command::Action(Action::with(ActionId::SortSet, &["name"])));
        assert_eq!(parse("gen"), Command::Action(Action::new(ActionId::GeneratorOpen)));
        assert_eq!(parse("gen 32"), Command::Generate { passphrase: false, size: Some(32) });
        assert_eq!(parse("gen phrase 6"), Command::Generate { passphrase: true, size: Some(6) });
        assert!(matches!(parse("gen lots"), Command::Message(MessageLevel::Error, _)));
        assert_eq!(
            parse("12"),
            Command::Action(Action::new(ActionId::NavBottom).counted(Some(12)))
        );
        assert_eq!(parse("trash"), Command::SetFilter(Filter::Trash));
    }

    #[test]
    fn names_and_paths_keep_their_spaces() {
        assert_eq!(
            parse("move Social media"),
            Command::Action(Action::with(ActionId::ItemMove, &["Social media"]))
        );
        assert_eq!(parse("move"), Command::Action(Action::new(ActionId::ItemMove)));
        assert_eq!(
            parse("folder new Work/Side projects"),
            Command::Action(Action::with(ActionId::FolderNew, &["Work/Side projects"]))
        );
        assert_eq!(parse("folder delete"), Command::Action(Action::new(ActionId::FolderDelete)));
        assert_eq!(
            parse("export csv ~/My backups/vault.csv"),
            Command::Action(Action::with(
                ActionId::VaultExport,
                &["csv", "~/My backups/vault.csv"]
            ))
        );
        assert_eq!(
            parse("import bitwardenjson  /tmp/an export.json "),
            Command::Action(Action::with(
                ActionId::VaultImport,
                &["bitwardenjson", "/tmp/an export.json"]
            ))
        );
    }

    #[test]
    fn parses_set_modes() {
        assert_eq!(
            parse("set theme=nord"),
            Command::Set { key: "theme".into(), value: Some("nord".into()), mode: SetMode::Set }
        );
        assert_eq!(
            parse("set animations!"),
            Command::Set { key: "animations".into(), value: None, mode: SetMode::Toggle }
        );
        assert_eq!(
            parse("set ui.theme?"),
            Command::Set { key: "ui.theme".into(), value: None, mode: SetMode::Show }
        );
        assert_eq!(
            parse("set animations&"),
            Command::Set { key: "animations".into(), value: None, mode: SetMode::Reset }
        );
    }

    #[test]
    fn unknown_commands_get_vim_error() {
        match parse("frobnicate") {
            Command::Message(level, text) => {
                assert_eq!(level, MessageLevel::Error);
                assert!(text.starts_with("E492:"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn comples_command_names() {
        let state = crate::app::tests::test_state();
        let candidates = completions("th", &state);
        assert_eq!(candidates, vec!["theme"]);
        let themes = completions("theme tok", &state);
        assert!(themes.contains(&"tokyonight-night".to_owned()));
        let keys = completions("set anim", &state);
        assert!(keys.contains(&"animations".to_owned()));
    }
}
