//! Export payloads, import merging and the password-health report.

use crate::state::{self, State};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Header of the Bitwarden CSV export, exactly as the real CLI writes it.
pub const CSV_HEADER: &str = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp";

/// Builds the content of an `export` command for the given format.
///
/// `password` is `Some` only for `--password`, which turns an
/// `encrypted_json` export into a password-protected one.
pub fn export_content(
    state: &State,
    format: &str,
    password: Option<&str>,
) -> Result<String, String> {
    match format {
        "json" => {
            let payload = json!({
                "encrypted": false,
                "folders": state.folders,
                "items": state.items,
            });
            serde_json::to_string(&payload).map_err(|e| format!("cannot serialise export: {e}"))
        }
        "csv" => Ok(export_csv(state)),
        "encrypted_json" => {
            let protected = password.is_some();
            let salt = if protected { Value::String(random_hex(16)) } else { Value::Null };
            let payload = json!({
                "encrypted": true,
                "passwordProtected": protected,
                "salt": salt,
                "data": random_hex(48),
            });
            serde_json::to_string(&payload).map_err(|e| format!("cannot serialise export: {e}"))
        }
        other => Err(format!("Unknown export format: {other}")),
    }
}

fn export_csv(state: &State) -> String {
    let mut output = String::from(CSV_HEADER);
    output.push('\n');
    for item in &state.items {
        let folder = item
            .get("folderId")
            .and_then(Value::as_str)
            .and_then(|id| folder_name(state, id))
            .unwrap_or_default();
        let login = item.get("login").and_then(Value::as_object);
        let login_uri = login
            .and_then(|login| login.get("uris"))
            .and_then(Value::as_array)
            .and_then(|uris| uris.first())
            .and_then(|uri| uri.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let login_username = login
            .and_then(|login| login.get("username"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let login_password = login
            .and_then(|login| login.get("password"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let login_totp =
            login.and_then(|login| login.get("totp")).and_then(Value::as_str).unwrap_or_default();
        let fields = match item.get("fields") {
            Some(Value::Array(fields)) => {
                serde_json::to_string(fields).unwrap_or_else(|_| "[]".to_owned())
            }
            _ => String::new(),
        };
        let favorite =
            if item.get("favorite").and_then(Value::as_bool).unwrap_or(false) { "1" } else { "0" };
        let kind = item.get("type").and_then(Value::as_i64).unwrap_or(1);
        let reprompt = item.get("reprompt").and_then(Value::as_i64).unwrap_or(0);
        let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
        let notes = item.get("notes").and_then(Value::as_str).unwrap_or_default();
        let row = [
            folder.to_owned(),
            favorite.to_owned(),
            kind.to_string(),
            name.to_owned(),
            notes.to_owned(),
            fields,
            reprompt.to_string(),
            login_uri.to_owned(),
            login_username.to_owned(),
            login_password.to_owned(),
            login_totp.to_owned(),
        ];
        let escaped: Vec<String> = row.iter().map(|value| csv_escape(value)).collect();
        output.push_str(&escaped.join(","));
        output.push('\n');
    }
    output
}

fn folder_name<'a>(state: &'a State, id: &str) -> Option<&'a str> {
    state
        .folders
        .iter()
        .find(|folder| folder.get("id").and_then(Value::as_str) == Some(id))
        .and_then(|folder| folder.get("name"))
        .and_then(Value::as_str)
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn random_hex(bytes: usize) -> String {
    let mut output = String::with_capacity(bytes * 2);
    for _ in 0..bytes {
        output.push_str(&format!("{:02x}", rand::random::<u8>()));
    }
    output
}

/// Imports a file into the vault, returning the number of items appended.
pub fn import_content(
    state: &mut State,
    format: &str,
    raw: &str,
    password: Option<&str>,
) -> Result<usize, String> {
    match format {
        "bitwardenjson" => {
            let parsed: Value =
                serde_json::from_str(raw).map_err(|e| format!("cannot parse import file: {e}"))?;
            let payload = unwrap_protected(&parsed, password)?;
            Ok(import_json_payload(state, &payload))
        }
        "bitwardenpasswordprotected" => {
            let parsed: Value =
                serde_json::from_str(raw).map_err(|e| format!("cannot parse import file: {e}"))?;
            let payload = unwrap_protected(&parsed, password)?;
            Ok(import_json_payload(state, &payload))
        }
        "bitwardencsv" => Ok(import_csv(state, raw)),
        _ => Ok(0),
    }
}

/// Accepts both a plain payload and the password-protected wrapper used by
/// fixtures: `{"encrypted":true,"passwordProtected":true,"password":"…","data":{…}}`.
fn unwrap_protected(file: &Value, password: Option<&str>) -> Result<Value, String> {
    let protected = file.get("passwordProtected").and_then(Value::as_bool).unwrap_or(false);
    if !protected {
        return Ok(file.clone());
    }
    let stored = file.get("password").and_then(Value::as_str);
    if stored != password {
        return Err("Invalid password.".to_owned());
    }
    match file.get("data") {
        Some(Value::Object(_)) => Ok(file.get("data").cloned().unwrap_or(Value::Null)),
        Some(Value::String(encoded)) => {
            serde_json::from_str(encoded).map_err(|e| format!("cannot parse import file: {e}"))
        }
        _ => Err("Cannot read the import file.".to_owned()),
    }
}

fn import_json_payload(state: &mut State, payload: &Value) -> usize {
    let mut folder_ids: HashMap<String, String> = HashMap::new();
    if let Some(folders) = payload.get("folders").and_then(Value::as_array) {
        for folder in folders {
            let Some(name) = folder.get("name").and_then(Value::as_str) else {
                continue;
            };
            let target = existing_folder_id(state, name).unwrap_or_else(|| {
                let id = state::new_id();
                state.folders.push(json!({"object": "folder", "id": id, "name": name}));
                id
            });
            if let Some(old_id) = folder.get("id").and_then(Value::as_str) {
                folder_ids.insert(old_id.to_owned(), target);
            }
        }
    }

    let Some(items) = payload.get("items").and_then(Value::as_array) else {
        return 0;
    };
    let mut imported = 0;
    for item in items {
        let Some(mut item) = item.as_object().cloned() else {
            continue;
        };
        if let Some(id) = item.get("id").and_then(Value::as_str)
            && state
                .items
                .iter()
                .any(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
        {
            continue;
        }
        if item.get("id").is_none_or(Value::is_null) {
            item.insert("id".to_owned(), Value::String(state::new_id()));
        }
        item.insert("object".to_owned(), Value::String("item".to_owned()));
        if let Some(folder_id) = item.get("folderId").and_then(Value::as_str) {
            let mapped = folder_ids.get(folder_id).cloned();
            item.insert("folderId".to_owned(), mapped.map_or(Value::Null, Value::String));
        }
        state.items.push(Value::Object(item));
        imported += 1;
    }
    imported
}

fn existing_folder_id(state: &State, name: &str) -> Option<String> {
    state
        .folders
        .iter()
        .find(|folder| folder.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|folder| folder.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn import_csv(state: &mut State, raw: &str) -> usize {
    let rows = parse_csv(raw);
    let Some((header, data)) = rows.split_first() else {
        return 0;
    };
    let index: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(position, name)| (name.trim().to_ascii_lowercase(), position))
        .collect();
    let mut imported = 0;
    for row in data {
        let field = |name: &str| -> String {
            index.get(name).and_then(|position| row.get(*position)).cloned().unwrap_or_default()
        };
        let name = field("name");
        if name.is_empty() {
            continue;
        }
        let kind = field("type").parse::<u8>().unwrap_or(1);
        let folder_name = field("folder");
        let folder_id = if folder_name.is_empty() {
            Value::Null
        } else {
            let id = existing_folder_id(state, &folder_name).unwrap_or_else(|| {
                let id = state::new_id();
                state.folders.push(json!({"object": "folder", "id": id, "name": folder_name}));
                id
            });
            Value::String(id)
        };
        let notes = field("notes");
        let mut item = Map::new();
        item.insert("id".to_owned(), Value::String(state::new_id()));
        item.insert("object".to_owned(), Value::String("item".to_owned()));
        item.insert("type".to_owned(), json!(kind));
        item.insert("name".to_owned(), Value::String(name));
        item.insert(
            "favorite".to_owned(),
            json!(matches!(field("favorite").as_str(), "1" | "true")),
        );
        item.insert("reprompt".to_owned(), json!(field("reprompt").parse::<u8>().unwrap_or(0)));
        item.insert("folderId".to_owned(), folder_id);
        item.insert(
            "notes".to_owned(),
            if notes.is_empty() { Value::Null } else { Value::String(notes) },
        );
        let fields = field("fields");
        if !fields.is_empty()
            && let Ok(parsed) = serde_json::from_str::<Value>(&fields)
        {
            item.insert("fields".to_owned(), parsed);
        }
        if kind == 1 {
            let uri = field("login_uri");
            let username = field("login_username");
            let password = field("login_password");
            let totp = field("login_totp");
            item.insert(
                "login".to_owned(),
                json!({
                    "uris": if uri.is_empty() { Value::Array(Vec::new()) } else { json!([{"uri": uri}]) },
                    "username": if username.is_empty() { Value::Null } else { json!(username) },
                    "password": if password.is_empty() { Value::Null } else { json!(password) },
                    "totp": if totp.is_empty() { Value::Null } else { json!(totp) },
                }),
            );
        } else if kind == 2 {
            item.insert("secureNote".to_owned(), json!({"type": 0}));
        }
        state.items.push(Value::Object(item));
        imported += 1;
    }
    imported
}

fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {}
            other => field.push(other),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Computes the `report password-health` payload from the state items.
pub fn password_health(state: &State, check_exposed: bool) -> Vec<Value> {
    let entries: Vec<(&Value, &str)> = state
        .items
        .iter()
        .filter_map(|item| {
            let password = item
                .get("login")
                .and_then(|login| login.get("password"))
                .and_then(Value::as_str)
                .filter(|password| !password.is_empty())?;
            Some((item, password))
        })
        .collect();

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (_, password) in &entries {
        *counts.entry(password).or_default() += 1;
    }

    entries
        .into_iter()
        .map(|(item, password)| {
            let reused = counts.get(password).copied().unwrap_or(1);
            let (exposed, exposed_count) = if !check_exposed {
                (Value::Null, Value::Null)
            } else {
                let is_exposed =
                    state.exposed_passwords.iter().any(|candidate| candidate == password);
                if is_exposed {
                    (Value::Bool(true), json!(3812))
                } else {
                    (Value::Bool(false), json!(0))
                }
            };
            json!({
                "object": "password-health",
                "id": item.get("id").cloned().unwrap_or(Value::Null),
                "name": item.get("name").cloned().unwrap_or(Value::Null),
                "passwordStrength": password_strength(password),
                "reuseCount": if reused > 1 { json!(reused) } else { Value::Null },
                "exposed": exposed,
                "exposedCount": exposed_count,
                "exposedError": Value::Null,
            })
        })
        .collect()
}

fn password_strength(password: &str) -> u8 {
    match password.chars().count() {
        0..=7 => 0,
        8..=11 => 1,
        12..=15 => 2,
        16..=19 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{STATUS_UNLOCKED, State};

    fn state_with_vault() -> State {
        State {
            status: STATUS_UNLOCKED.to_owned(),
            folders: vec![json!({"object": "folder", "id": "f1", "name": "Work"})],
            items: vec![
                json!({
                    "object": "item", "id": "i1", "type": 1, "name": "One", "favorite": true,
                    "reprompt": 0, "folderId": "f1",
                    "login": {"username": "u1", "password": "short", "uris": [{"uri": "https://one.example"}], "totp": null},
                }),
                json!({
                    "object": "item", "id": "i2", "type": 2, "name": "Note, with comma",
                    "notes": "line one\nline two", "secureNote": {"type": 0},
                }),
            ],
            ..State::default()
        }
    }

    #[test]
    fn plain_json_export_has_plaintext_flags_and_raw_objects() {
        let state = state_with_vault();
        let exported = export_content(&state, "json", None).expect("export");
        let parsed: Value = serde_json::from_str(&exported).expect("json");
        assert_eq!(parsed["encrypted"], false);
        assert_eq!(parsed["folders"][0]["name"], "Work");
        assert_eq!(parsed["items"][0]["id"], "i1");
    }

    #[test]
    fn encrypted_json_export_flags_depend_on_password() {
        let state = state_with_vault();
        let plain: Value =
            serde_json::from_str(&export_content(&state, "encrypted_json", None).expect("export"))
                .expect("json");
        assert_eq!(plain["encrypted"], true);
        assert_eq!(plain["passwordProtected"], false);
        assert!(plain["salt"].is_null());

        let protected: Value = serde_json::from_str(
            &export_content(&state, "encrypted_json", Some("pw")).expect("export"),
        )
        .expect("json");
        assert_eq!(protected["encrypted"], true);
        assert_eq!(protected["passwordProtected"], true);
        assert!(protected["salt"].is_string());
        assert!(protected["data"].is_string());
    }

    #[test]
    fn csv_export_has_the_documented_header_and_one_row_per_item() {
        let state = state_with_vault();
        let csv = export_content(&state, "csv", None).expect("export");
        let rows = parse_csv(&csv);
        assert_eq!(rows[0][0], "folder");
        assert_eq!(rows.len(), 3);
        assert!(csv.contains("\"Note, with comma\""));
        assert!(csv.contains("https://one.example"));
    }

    #[test]
    fn import_appends_items_and_skips_duplicate_ids() {
        let mut state = state_with_vault();
        let payload = json!({
            "encrypted": false,
            "folders": [{"id": "old-folder", "name": "Imported"}],
            "items": [
                {"id": "i1", "type": 1, "name": "Duplicate"},
                {"id": "new-1", "type": 1, "name": "Fresh", "folderId": "old-folder"},
                {"type": 2, "name": "No id"},
            ],
        });
        let imported = import_json_payload(&mut state, &payload);
        assert_eq!(imported, 2);
        assert_eq!(state.items.len(), 4);
        assert!(state.items.iter().any(|item| item["name"] == "Fresh"));
        assert_eq!(state.folders.len(), 2);
        let fresh = state.items.iter().find(|item| item["name"] == "Fresh").expect("fresh");
        let new_folder =
            state.folders.iter().find(|folder| folder["name"] == "Imported").expect("folder");
        assert_eq!(fresh["folderId"], new_folder["id"]);
        let generated = state.items.iter().find(|item| item["name"] == "No id").expect("generated");
        assert!(generated["id"].as_str().is_some_and(|id| id.len() == 36));
    }

    #[test]
    fn password_protected_import_validates_the_password() {
        let mut state = State::default();
        let raw = r#"{"encrypted":true,"passwordProtected":true,"password":"letmein","data":{"items":[{"id":"x","type":1,"name":"Secret"}],"folders":[]}}"#;
        assert!(
            import_content(&mut state, "bitwardenpasswordprotected", raw, Some("wrong")).is_err()
        );
        let imported =
            import_content(&mut state, "bitwardenpasswordprotected", raw, Some("letmein"))
                .expect("import");
        assert_eq!(imported, 1);
        assert_eq!(state.items[0]["name"], "Secret");
    }

    #[test]
    fn csv_import_creates_logins_and_folders() {
        let mut state = State::default();
        let raw = format!(
            "{CSV_HEADER}\nNew Folder,1,1,My Login,note,,\
             0,https://example.com,user,pass,\n"
        );
        let imported = import_content(&mut state, "bitwardencsv", &raw, None).expect("import");
        assert_eq!(imported, 1);
        assert_eq!(state.items[0]["name"], "My Login");
        assert_eq!(state.items[0]["login"]["username"], "user");
        assert_eq!(state.folders[0]["name"], "New Folder");
        assert_eq!(state.items[0]["folderId"], state.folders[0]["id"]);
    }

    #[test]
    fn unknown_import_formats_merge_nothing() {
        let mut state = state_with_vault();
        let imported = import_content(&mut state, "keepass2xml", "<xml/>", None).expect("import");
        assert_eq!(imported, 0);
        assert_eq!(state.items.len(), 2);
    }

    #[test]
    fn password_health_scores_strength_reuse_and_exposure() {
        let state = State {
            exposed_passwords: vec!["the same password".to_owned()],
            items: vec![
                json!({"id": "a", "type": 1, "name": "A", "login": {"password": "the same password"}}),
                json!({"id": "b", "type": 1, "name": "B", "login": {"password": "the same password"}}),
                json!({"id": "c", "type": 1, "name": "C", "login": {"password": "short"}}),
                json!({"id": "d", "type": 2, "name": "Note"}),
            ],
            ..State::default()
        };
        let report = password_health(&state, true);
        assert_eq!(report.len(), 3);
        assert_eq!(report[0]["object"], "password-health");
        assert_eq!(report[0]["passwordStrength"], 3);
        assert_eq!(report[0]["reuseCount"], 2);
        assert_eq!(report[0]["exposed"], true);
        assert_eq!(report[0]["exposedCount"], 3812);
        assert_eq!(report[2]["passwordStrength"], 0);
        assert!(report[2]["reuseCount"].is_null());
        assert_eq!(report[2]["exposed"], false);
    }

    #[test]
    fn password_health_can_skip_exposure_checks() {
        let state = State {
            items: vec![
                json!({"id": "a", "type": 1, "name": "A", "login": {"password": "abcdefgh"}}),
            ],
            ..State::default()
        };
        let report = password_health(&state, false);
        assert!(report[0]["exposed"].is_null());
        assert!(report[0]["exposedCount"].is_null());
        assert_eq!(report[0]["passwordStrength"], 1);
    }
}
