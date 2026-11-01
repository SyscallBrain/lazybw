//! Command-line surface of the `fake-bw` test double.

use crate::import_export;
use crate::serve;
use crate::state::{
    self, DEFAULT_VERSION, STATUS_LOCKED, STATUS_UNAUTHENTICATED, STATUS_UNLOCKED, State,
};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Formats printed by `import --formats`.
pub const IMPORT_FORMATS: [&str; 15] = [
    "bitwardenjson",
    "bitwardenpasswordprotected",
    "bitwardencsv",
    "1password1pux",
    "1password1pif",
    "chromecsv",
    "firefoxcsv",
    "keepass2xml",
    "lastpasscsv",
    "safaricsv",
    "keepercsv",
    "dashlanecsv",
    "bitwardentxt",
    "msecurecsv",
    "passwordbossjson",
];

const EXPORT_FORMATS: [&str; 3] = ["json", "csv", "encrypted_json"];

/// Runs one command and returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let Some(command) = args.first() else {
        print_usage();
        return 0;
    };
    match command.as_str() {
        "--version" | "-V" | "version" => print_version(),
        "--help" | "-h" | "help" => {
            print_usage();
            0
        }
        "status" => with_state(args, |state, _path| status(state)),
        "login" => with_state(args, |state, _path| login(state, &Parsed::parse(&args[1..]))),
        "logout" => with_state(args, |state, _path| logout(state)),
        "config" => {
            with_state(args, |state, _path| config_server(state, &Parsed::parse(&args[1..])))
        }
        "export" => with_state(args, |state, _path| export(state, &Parsed::parse(&args[1..]))),
        "import" => with_state(args, |state, _path| import(state, &Parsed::parse(&args[1..]))),
        "report" => with_state(args, |state, _path| report(state, &Parsed::parse(&args[1..]))),
        "serve" => serve::run(args),
        "gen-fixture" => gen_fixture(&Parsed::parse(&args[1..])),
        "dump-state" => dump_state(),
        "reset" => with_state(args, |state, _path| reset(state)),
        other => {
            eprintln!("Unknown command: {other}");
            2
        }
    }
}

fn with_state(args: &[String], command: impl FnOnce(&mut State, &Path) -> i32) -> i32 {
    let path = match state::state_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    let mut state = match State::load(&path) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    state.record_invocation(args);
    let code = command(&mut state, &path);
    if let Err(error) = state.save(&path) {
        eprintln!("{error}");
        return 1;
    }
    code
}

fn print_version() -> i32 {
    let version = state::state_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<State>(&raw).ok())
        .map(|state| state.version)
        .unwrap_or_else(|| DEFAULT_VERSION.to_owned());
    println!("{version}");
    0
}

fn print_usage() {
    println!(
        "fake-bw: test double for the Bitwarden CLI\n\
         \n\
         Commands:\n\
         \x20 --version\n\
         \x20 status --response\n\
         \x20 login [--raw|--sso|--apikey] [--response]\n\
         \x20 logout [--response]\n\
         \x20 config server <url> [--response]\n\
         \x20 export --format <json|csv|encrypted_json> [--raw] [--output PATH] [--password]\n\
         \x20 import <format> <path> [--passwordenv VAR] [--response]\n\
         \x20 import --formats\n\
         \x20 report password-health [--no-check-exposed] [--response]\n\
         \x20 serve --hostname fd+connected://N | unix://PATH\n\
         \x20 gen-fixture --items N\n\
         \x20 dump-state\n\
         \x20 reset"
    );
}

fn status(state: &State) -> i32 {
    print_envelope(&json!({
        "success": true,
        "data": {
            "object": "template",
            "template": {
                "serverUrl": state.server_url,
                "lastSync": state.last_sync,
                "userEmail": state.email,
                "userId": state.user_id,
                "status": state.status,
            },
        },
    }));
    0
}

fn login(state: &mut State, parsed: &Parsed) -> i32 {
    let raw = parsed.has("--raw");
    if parsed.has("--apikey") {
        let client_id = env_nonempty("BW_CLIENTID");
        let client_secret = env_nonempty("BW_CLIENTSECRET");
        if client_id.is_none() || client_secret.is_none() {
            return fail("Invalid API key.");
        }
    } else if !parsed.has("--sso") {
        write_stderr("? Email address: ");
        let email = read_line();
        write_stderr("? Master password: [hidden] ");
        let password = read_line();
        if state.two_factor {
            write_stderr("? Two-step login code: ");
            let _code = read_line();
        }
        if password != state.master_password {
            return fail("Invalid master password.");
        }
        if !email.trim().is_empty() {
            state.email = Some(email.trim().to_owned());
        }
    }

    state.status = STATUS_LOCKED.to_owned();
    state.session_key = Some(state.session_key());
    let session = state.session_key();
    if raw {
        println!("{session}");
    } else {
        print_envelope(&json!({
            "success": true,
            "data": {
                "object": "message",
                "title": "Logged in",
                "message": "You are now logged in!",
                "raw": session,
            },
        }));
    }
    0
}

fn logout(state: &mut State) -> i32 {
    state.status = STATUS_UNAUTHENTICATED.to_owned();
    state.session_key = None;
    print_envelope(&json!({
        "success": true,
        "data": {
            "object": "message",
            "title": "Logged out",
            "message": "You have logged out.",
            "raw": Value::Null,
        },
    }));
    0
}

fn config_server(state: &mut State, parsed: &Parsed) -> i32 {
    if parsed.positionals.first().map(String::as_str) != Some("server") {
        return fail("Missing server URL.");
    }
    let Some(url) = parsed.positionals.get(1) else {
        return fail("Missing server URL.");
    };
    if state.status != STATUS_UNAUTHENTICATED {
        return fail("Logout required before server config update.");
    }
    state.server_url = Some(url.clone());
    print_envelope(&json!({
        "success": true,
        "data": {
            "object": "message",
            "title": "Server URL set",
            "message": format!("Server URL set to {url}."),
            "raw": Value::Null,
        },
    }));
    0
}

fn export(state: &mut State, parsed: &Parsed) -> i32 {
    if !state.is_unlocked() {
        return auth_failure(state);
    }
    let Some(format) = parsed.value("--format") else {
        return fail("Missing --format.");
    };
    if !EXPORT_FORMATS.contains(&format.as_str()) {
        return fail("Unknown export format.");
    }
    let raw = parsed.has("--raw");
    let output = parsed.value("--output");
    if !raw && output.is_none() {
        return fail("Missing --output.");
    }
    let password = if parsed.has("--password") {
        write_stdout("? Export file password: ");
        Some(read_line())
    } else {
        None
    };
    let content = match import_export::export_content(state, &format, password.as_deref()) {
        Ok(content) => content,
        Err(error) => return fail(&error),
    };
    if let Some(path) = output
        && let Err(error) = write_private(&path, &content)
    {
        return fail(&error);
    }
    if raw {
        print!("{content}");
    } else {
        print_envelope(&json!({
            "success": true,
            "data": {
                "object": "message",
                "title": "Exported",
                "message": format!("Exported {format} data."),
                "raw": Value::Null,
            },
        }));
    }
    0
}

fn import(state: &mut State, parsed: &Parsed) -> i32 {
    if parsed.has("--formats") {
        if !state.is_logged_in() {
            return fail("You are not logged in.");
        }
        for format in IMPORT_FORMATS {
            println!("{format}");
        }
        return 0;
    }
    if !state.is_unlocked() {
        return auth_failure(state);
    }
    let Some(format) = parsed.positionals.first() else {
        return fail("Missing import format.");
    };
    let Some(path) = parsed.positionals.get(1) else {
        return fail("Missing import file.");
    };
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => return fail(&format!("Cannot read the import file: {error}")),
    };
    let password = match parsed.value("--passwordenv") {
        Some(variable) => match env_nonempty(&variable) {
            Some(value) => Some(value),
            None => return fail("Invalid password."),
        },
        None => None,
    };
    let imported = match import_export::import_content(state, format, &raw, password.as_deref()) {
        Ok(imported) => imported,
        Err(error) => return fail(&error),
    };
    print_envelope(&json!({
        "success": true,
        "data": {
            "object": "message",
            "title": "Imported",
            "message": format!("Imported {imported} item(s) from {path}"),
            "raw": Value::Null,
        },
    }));
    0
}

fn report(state: &mut State, parsed: &Parsed) -> i32 {
    if !state.is_unlocked() {
        return auth_failure(state);
    }
    let check_exposed = !parsed.has("--no-check-exposed");
    let rows = import_export::password_health(state, check_exposed);
    // The real CLI wraps the rows in a list response.
    print_envelope(&json!({"success": true, "data": {"object": "list", "data": rows}}));
    0
}

fn auth_failure(state: &State) -> i32 {
    if state.status == STATUS_UNAUTHENTICATED {
        fail("You are not logged in.")
    } else {
        fail("Vault is locked.")
    }
}

fn fail(message: &str) -> i32 {
    print_envelope(&json!({"success": false, "message": message, "data": null}));
    1
}

fn print_envelope(value: &Value) {
    match serde_json::to_string(value) {
        Ok(text) => println!("{text}"),
        Err(_) => {
            println!("{{\"success\":false,\"message\":\"Serialisation error\",\"data\":null}}")
        }
    }
}

fn write_private(path: &str, content: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("Cannot write the export file: {error}"))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("Cannot write the export file: {error}"))?;
    file.sync_all().map_err(|error| format!("Cannot write the export file: {error}"))?;
    let _ = fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
    Ok(())
}

fn write_stderr(message: &str) {
    let _ = write!(std::io::stderr(), "{message}");
    let _ = std::io::stderr().flush();
}

fn write_stdout(message: &str) {
    let _ = write!(std::io::stdout(), "{message}");
    let _ = std::io::stdout().flush();
}

fn read_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim_end_matches(['\r', '\n']).to_owned()
}

fn env_nonempty(variable: &str) -> Option<String> {
    std::env::var(variable).ok().filter(|value| !value.is_empty())
}

fn dump_state() -> i32 {
    let path = match state::state_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    match fs::read_to_string(&path) {
        Ok(raw) => {
            print!("{raw}");
            if !raw.ends_with('\n') {
                println!();
            }
            0
        }
        Err(error) => {
            eprintln!("cannot read state file {}: {error}", path.display());
            1
        }
    }
}

fn reset(state: &mut State) -> i32 {
    state.log.clear();
    state.last_sync = None;
    0
}

fn gen_fixture(parsed: &Parsed) -> i32 {
    let items =
        parsed.value("--items").and_then(|value| value.parse::<usize>().ok()).unwrap_or(100);
    let fixture = fixture_state(items);
    match serde_json::to_string_pretty(&fixture) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(_) => 1,
    }
}

fn fixture_state(items: usize) -> State {
    let mut rng = StdRng::seed_from_u64(items as u64);
    let folders: Vec<Value> = (0..5)
        .map(|index| {
            json!({
                "object": "folder",
                "id": format!("10000000-0000-4000-8000-{index:012x}"),
                "name": format!("Folder {}", index + 1),
            })
        })
        .collect();
    let folder_ids: Vec<String> = folders
        .iter()
        .map(|folder| folder.get("id").and_then(Value::as_str).unwrap_or_default().to_owned())
        .collect();

    let timestamp = "2026-01-01T00:00:00.000Z";
    let mut generated = Vec::with_capacity(items);
    for index in 0..items {
        let folder_id = if index % 4 == 0 {
            Value::Null
        } else if let Some(id) = folder_ids.get(index % folder_ids.len()) {
            json!(id)
        } else {
            Value::Null
        };
        let favorite = rng.random_bool(0.2);
        let id = format!("00000000-0000-4000-8000-{index:012x}");
        let item = match index % 3 {
            0 => json!({
                "object": "item", "id": id, "type": 1,
                "name": format!("Login {index}"),
                "favorite": favorite, "reprompt": 0, "folderId": folder_id,
                "creationDate": timestamp, "revisionDate": timestamp,
                "deletedDate": null, "archivedDate": null,
                "login": {
                    "username": format!("user{index}@example.com"),
                    "password": format!("fixture-password-{index}"),
                    "uris": [{"uri": format!("https://site{index}.example")}],
                },
                "fields": [],
                "passwordHistory": null,
            }),
            1 => json!({
                "object": "item", "id": id, "type": 3,
                "name": format!("Card {index}"),
                "favorite": favorite, "reprompt": 0, "folderId": folder_id,
                "creationDate": timestamp, "revisionDate": timestamp,
                "deletedDate": null, "archivedDate": null,
                "card": {
                    "cardholderName": format!("Holder {index}"),
                    "brand": "Visa",
                    "number": format!("41111111111{index:04}"),
                    "expMonth": "1", "expYear": "2030", "code": format!("{:03}", index % 1000),
                },
                "fields": [],
            }),
            _ => json!({
                "object": "item", "id": id, "type": 2,
                "name": format!("Note {index}"),
                "favorite": favorite, "reprompt": 0, "folderId": folder_id,
                "creationDate": timestamp, "revisionDate": timestamp,
                "deletedDate": null, "archivedDate": null,
                "secureNote": {"type": 0},
                "notes": format!("Fixture note {index}"),
                "fields": [],
            }),
        };
        generated.push(item);
    }

    State { status: STATUS_UNLOCKED.to_owned(), folders, items: generated, ..State::default() }
}

/// The parsed form of a subcommand's argument list.
#[derive(Debug, Default)]
struct Parsed {
    flags: Vec<String>,
    values: Vec<(String, String)>,
    positionals: Vec<String>,
}

const VALUE_FLAGS: [&str; 6] =
    ["--format", "--output", "--passwordenv", "--items", "--hostname", "--session"];

impl Parsed {
    fn parse(args: &[String]) -> Self {
        let mut parsed = Self::default();
        let mut index = 0;
        while index < args.len() {
            let argument = &args[index];
            if let Some(body) = argument.strip_prefix("--") {
                if let Some((name, value)) = body.split_once('=') {
                    parsed.values.push((format!("--{name}"), value.to_owned()));
                } else if VALUE_FLAGS.contains(&argument.as_str()) {
                    if let Some(value) = args.get(index + 1) {
                        parsed.values.push((argument.clone(), value.clone()));
                        index += 1;
                    }
                } else {
                    parsed.flags.push(argument.clone());
                }
            } else {
                parsed.positionals.push(argument.clone());
            }
            index += 1;
        }
        parsed
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|candidate| candidate == flag)
    }

    fn value(&self, flag: &str) -> Option<String> {
        self.values.iter().find(|(name, _value)| name == flag).map(|(_name, value)| value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parser_separates_flags_values_and_positionals() {
        let parsed = Parsed::parse(&args(&[
            "byte",
            "/tmp/in.json",
            "--passwordenv",
            "IMPORT_PW",
            "--response",
        ]));
        assert_eq!(parsed.positionals, vec!["byte", "/tmp/in.json"]);
        assert_eq!(parsed.value("--passwordenv").as_deref(), Some("IMPORT_PW"));
        assert!(parsed.has("--response"));
    }

    #[test]
    fn parser_accepts_equals_form() {
        let parsed = Parsed::parse(&args(&["--format=json", "--length"]));
        assert_eq!(parsed.value("--format").as_deref(), Some("json"));
        assert!(parsed.positionals.is_empty());
    }

    #[test]
    fn fixture_is_deterministic_and_has_the_requested_items() {
        let first = fixture_state(12);
        let second = fixture_state(12);
        assert_eq!(first.items, second.items);
        assert_eq!(first.folders, second.folders);
        assert_eq!(first.items.len(), 12);
        assert_eq!(first.status, STATUS_UNLOCKED);
        assert!(first.items.iter().any(|item| item["type"] == 1));
        assert!(first.items.iter().any(|item| item["type"] == 2));
        assert!(first.items.iter().any(|item| item["type"] == 3));
    }

    #[test]
    fn fixture_ids_are_unique() {
        let fixture = fixture_state(200);
        let mut ids: Vec<&str> =
            fixture.items.iter().filter_map(|item| item["id"].as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 200);
    }
}
