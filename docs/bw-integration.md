# lazybw — Integration with the Bitwarden CLI

Everything in this document was taken from the source code of `bitwarden/clients` (`apps/cli`,
`libs/common`) at CLI version **2026.10.1** and from the published npm packages of older versions.
Where a behaviour still needs confirmation with a real account, it is marked **[verify]** and listed in
the manual checklist (`scripts/verify-real-bw.md`).

---

## 1. Requirements, versions and capabilities

| bw version | What it enables in lazybw |
|---|---|
| **≥ 2025.9.0** (required) | `bw serve --hostname fd+connected://N` and `unix://PATH` |
| ≥ 2025.10.0 | `list items --archived` (lazybw shows archived items read-only if any exist) |
| ≥ 2026.5.0 | Item types 6 (bank account), 7 (driver's licence), 8 (passport) may appear, behind a server feature flag. Read-only generic display. |
| ≥ 2026.8.0 | `bw report password-health` (strength, reuse, exposure via Have I Been Pwned) |

```rust
pub struct Capabilities {
    pub version: semver::Version,
    pub archived_list: bool,     // >= 2025.10.0
    pub new_item_types: bool,    // >= 2026.5.0
    pub password_health: bool,   // >= 2026.8.0
}
```

`bw --version` prints e.g. `2026.10.1`. Parse with `semver` (versions are `YYYY.M.P`). Below 2025.9.0,
show the "bw too old" error screen.

### Premium gates (the user has no Premium)

For personal items without Premium the CLI refuses: `bw get totp` ("Premium status is required to use
this feature."), attachment download/upload/delete, `bw archive`, and editing an archived item (it
prompts to unarchive). lazybw **never calls** these. TOTP codes are computed locally (`src/totp.rs`).

## 2. Locating `bw` and environment hygiene

Resolution order: config `bw.path` → env `LAZYBW_BW` → `which("bw")`. Not found → error screen with
"Install the Bitwarden CLI: `npm install -g @bitwarden/cli` or your distribution's package" and the path
searched.

Environment for every child process:

- Start from the parent environment.
- Remove `BW_SESSION` unless `bw.inherit_session = true` (then it is used for the first serve start).
- Keep `BITWARDENCLI_APPDATA_DIR` if set (custom data dir).
- Set `NO_COLOR=1`.
- Non-interactive commands: add `--nointeraction` **and** `--response` (JSON envelope on stdout).
- Commands that need the unlocked vault: set `BW_SESSION=<session key>` in the child **environment**
  (never `--session` on argv).
- Ignore stderr content for parsing (the CLI prints warnings such as "Unable to fetch ServerConfig");
  log it at `debug`.
- In the child (via `pre_exec`): `prctl(PR_SET_PDEATHSIG, SIGTERM)`.

## 3. Session state machine

```
            ┌───────────┐  bw missing / too old / config error
 launch ──▶ │ Starting  │────────────────────────────────────────▶ Error (terminal state, q quits)
            └─────┬─────┘
                  │ bw status --response
     unauthenticated│                 locked │ unlocked (only if inherit_session)
                  ▼                        ▼
            ┌───────────┐  login ok  ┌───────────┐  POST /unlock ok   ┌───────────┐
            │ LoggedOut │──────────▶ │  Locked   │──────────────────▶ │ Loading   │
            │ (Login UI)│            │(Unlock UI)│◀── lock/auto-lock ─│ → Ready   │
            └─────▲─────┘            └─────┬─────┘                    └─────┬─────┘
                  │        logout          │                                │ "Vault is locked." from bw
                  └────────────────────────┴────────────────────────────────┘ → Locked
 Any state with a running serve: child exit → Restarting → (Locked | Ready) or Error after 2 failures
```

`Ready` sub-states are driven by UI (screens, overlays), not by the backend.

## 4. Startup sequence

1. Load config and theme; draw the Starting screen ("Starting Bitwarden CLI…").
2. Locate `bw`, run `bw --version` → capabilities.
3. Run `bw status --response` → `status` ∈ `unauthenticated | locked | unlocked`, `serverUrl`, `userEmail`,
   `lastSync`.
4. `unauthenticated` → Login screen (§8.2, §8.3). After a successful login with a session key go to step 5
   passing `BW_SESSION`.
5. Spawn `bw serve` (§5), wait for readiness with `GET /status`.
6. `GET /status` says `unlocked` → load the vault (§6.3 batch) → Ready. `locked` → Unlock screen.

## 5. `bw serve` over a socketpair

Facts from `apps/cli/src/commands/serve.command.ts`:

- `--hostname fd+connected://N` treats fd `N` as **one already-connected socket**, emits it as a connection
  on an `http.Server`, disables timeouts, and **stops serving when that socket closes**.
- Origin protection is on by default: requests with an `Origin` header are rejected (403), and the `Host`
  header must be one of `localhost:<port>`, `127.0.0.1:<port>`, `[::1]:<port>`, `<hostname>:<port>`, where
  `<port>` defaults to **8087**. lazybw always sends `Host: localhost:8087` and never sends `Origin`.
- The server sets `BW_SERVE=true` and `BW_NOINTERACTION=true` internally.
- `bw serve` exits with "You are not logged in." if the account is unauthenticated, so login must happen
  first.

Spawn sketch (adapt to the exact `command-fds` API; it supports `tokio::process::Command` with its
`tokio` feature — verify):

```rust
let (ours, theirs) = std::os::unix::net::UnixStream::pair()?;
let mut cmd = tokio::process::Command::new(&bw_path);
cmd.args(["serve", "--hostname", "fd+connected://3"])
    .envs(child_env(session_key.as_ref()))     // §2
    .stdin(Stdio::null())
    .stdout(Stdio::piped())                     // → log at debug
    .stderr(Stdio::piped())                     // → log at debug
    .kill_on_drop(true);
cmd.fd_mappings(vec![FdMapping { parent_fd: theirs.into(), child_fd: 3 }])?;
unsafe { cmd.pre_exec(set_pdeathsig_sigterm) }; // SAFETY: async-signal-safe prctl only
let child = cmd.spawn()?;
ours.set_nonblocking(true)?;
let io = TokioIo::new(tokio::net::UnixStream::from_std(ours)?);
let (sender, conn) = hyper::client::conn::http1::handshake(io).await?;
tokio::spawn(async move { if let Err(e) = conn.await { tracing::debug!(?e, "serve connection ended") } });
```

- Readiness: `GET /status` with retries (100 ms, ×1.5 backoff, total 30 s). Requests written before the
  server attaches wait in the socket buffer; still use the retry loop.
- One connection, so one request at a time. `BwService` queues requests (FIFO) and awaits
  `sender.ready()` before each.
- Fallback transport (config `bw.transport = "unix"`): `--hostname unix://$XDG_RUNTIME_DIR/lazybw/bw.sock`
  with the directory at mode 0700 and the socket path unique per process; connect with
  `tokio::net::UnixStream::connect`. Same headers. Implement and test with `fake-bw`.

## 6. HTTP contract

### 6.1 Envelope

Every JSON response has this shape (from `models/response.ts`):

```json
{ "success": true,  "data": { ... } }
{ "success": false, "message": "Vault is locked.", "data": null }
```

Failures use HTTP **400**. A 403 means the Host/Origin check failed (a lazybw bug). `data.object` names
the payload kind: `"list"`, `"item"`, `"folder"`, `"message"`, `"string"`, `"template"`.

Required request headers: `Host: localhost:8087`, `Accept: application/json`, and for bodies
`Content-Type: application/json`. Bodies are plain JSON (not base64; base64 is only for the CLI argv form).

Boolean query options are parsed with `convertBooleanOption`: present-and-empty or `"true"` = true.
Always send `name=true`.

### 6.2 Endpoints used by lazybw

| Purpose | Method and path | Body | `data` on success | Timeout |
|---|---|---|---|---|
| Server/session status | `GET /status` | — | `{object:"template", template:{serverUrl, lastSync, userEmail, userId, status}}` | 10 s |
| Unlock | `POST /unlock` | `{"password": "..."}` | `{object:"message", title, message, raw: "<session key>"}` | 120 s |
| Lock | `POST /lock` | — | message | 10 s |
| Sync | `POST /sync` (`?force=true`) | — | message ("Syncing complete.") | 120 s |
| List items | `GET /list/object/items` | — | `{object:"list", data:[Item]}` (excludes trash) | 60 s |
| List trash | `GET /list/object/items?trash=true` | — | list of Items with `deletedDate` | 60 s |
| List archived | `GET /list/object/items?archived=true` (cap.) | — | list | 60 s |
| List folders | `GET /list/object/folders` | — | list of `{object:"folder", id, name}`; includes a pseudo entry `id: null, name: "No Folder"` → **skip it** | 30 s |
| Get item | `GET /object/item/{id}` | — | Item | 30 s |
| Create item | `POST /object/item` | Item without `id` | Item | 30 s |
| Edit item | `PUT /object/item/{id}` | **full** Item | Item | 30 s |
| Trash item | `DELETE /object/item/{id}` | — | `null` | 30 s |
| Delete permanently | `DELETE /object/item/{id}?permanent=true` | — | `null` | 30 s |
| Restore from trash | `POST /restore/item/{id}` | — | `null` | 30 s |
| Create folder | `POST /object/folder` | `{"name": "Work/Projects"}` | Folder | 30 s |
| Rename folder | `PUT /object/folder/{id}` | `{"name": "..."}` | Folder | 30 s |
| Delete folder | `DELETE /object/folder/{id}` | — | `null` (items move to "No folder" server-side) | 30 s |
| Generate | `GET /generate?...` | — | `{object:"string", data:"<password>"}` | 10 s |

`/generate` query parameters (from `tools/generate.command.ts`):

| Param | Meaning |
|---|---|
| `uppercase`, `lowercase`, `number`, `special` | character classes (booleans) |
| `length` | password length (min 5; lazybw UI range 5–128) |
| `minNumber`, `minSpecial` | minimum counts |
| `ambiguous` | **true = avoid** ambiguous characters (the command inverts it internally) |
| `passphrase` | boolean: generate a passphrase |
| `words` | number of words (min 3; UI range 3–20) |
| `separator` | word separator; special values `space` and `empty` |
| `capitalize`, `includeNumber` | passphrase options |

Endpoints that exist but lazybw must not use: `/move`, `/confirm`, `/attachment`, `/send/*`,
`/archive/*`, `/object/template/*`, `/object/totp/*`, `/object/attachment/*`.

### 6.3 Loading the vault

After unlock or sync, issue in this order: folders → items → trash → (archived if capability). Build the
cache only when all succeed; on failure keep the previous cache and show the error.

### 6.4 Edit semantics (critical)

The CLI applies a PUT body with `CipherExport.toView(body, existingView)`: fields missing from the body
are **cleared**, not kept. Therefore:

- Always send the complete item, starting from the latest cached version (refresh with `GET` first if
  the cache is older than the last sync).
- Preserve unknown fields at every level (`#[serde(flatten)] extra`).
- Never drop `login.fido2Credentials`, `passwordHistory`, `fields`, `attachments` or dates.
- Changing `login.password` makes the server append the old one to `passwordHistory` (max 5 entries).

## 7. Data models

Field names and enums from `libs/common/src/models/export/*.ts` and `vault/enums`. All models:
`#[serde(rename_all = "camelCase")]`, every field optional unless stated, and a flattened
`extra: serde_json::Map<String, Value>`. **Keep dates as `String`** (exactly as received) and parse them
lazily for display; never re-serialise a date in a different format.

```rust
pub struct Item {
    pub id: Option<String>,
    pub organization_id: Option<String>,   // always None for this user; preserve anyway
    pub folder_id: Option<String>,
    #[serde(rename = "type")] pub kind: ItemType,      // integer
    pub name: String,
    pub notes: Option<Sensitive>,
    pub favorite: bool,                    // default false
    pub reprompt: u8,                      // 0 none, 1 master password
    pub fields: Option<Vec<Field>>,
    pub login: Option<Login>,
    pub secure_note: Option<SecureNote>,
    pub card: Option<Card>,
    pub identity: Option<Identity>,
    pub ssh_key: Option<SshKey>,
    pub password_history: Option<Vec<PasswordHistory>>,
    pub attachments: Option<Vec<Attachment>>,     // read-only display
    pub collection_ids: Option<Vec<String>>,
    pub revision_date: Option<String>,
    pub creation_date: Option<String>,
    pub deleted_date: Option<String>,
    pub archived_date: Option<String>,
    pub object: Option<String>,            // "item" in responses; omit when sending
    #[serde(flatten)] pub extra: Map<String, Value>,   // bankAccount, driversLicense, passport, future
}

pub enum ItemType { Login = 1, SecureNote = 2, Card = 3, Identity = 4, SshKey = 5, Other(u8) }

pub struct Login {
    pub uris: Option<Vec<LoginUri>>,
    pub username: Option<Sensitive>,
    pub password: Option<Sensitive>,
    pub totp: Option<Sensitive>,
    pub password_revision_date: Option<String>,
    pub fido2_credentials: Option<Vec<Value>>,  // opaque; display rpId, userName, creationDate
    #[serde(flatten)] pub extra: Map<String, Value>,
}
pub struct LoginUri { pub uri: Option<String>, pub r#match: Option<u8>, /* uriChecksum etc. in extra */ }
// match: null = Default, 0 Base domain, 1 Host, 2 Starts with, 3 Exact, 4 Regular expression, 5 Never

pub struct Field { pub name: Option<String>, pub value: Option<Sensitive>, #[serde(rename="type")] pub kind: u8, pub linked_id: Option<u16> }
// type: 0 Text, 1 Hidden, 2 Boolean ("true"/"false"), 3 Linked (value null, linkedId set)
// linkedId: Login 100 Username, 101 Password;
//   Card 300 Cardholder name, 301 Exp month, 302 Exp year, 303 Code, 304 Brand, 305 Number;
//   Identity 400 Title, 401 Middle name, 402 Address 1, 403 Address 2, 404 Address 3, 405 City,
//   406 State, 407 Postal code, 408 Country, 409 Company, 410 Email, 411 Phone, 412 SSN,
//   413 Username, 414 Passport number, 415 License number, 416 First name, 417 Last name, 418 Full name

pub struct SecureNote { #[serde(rename = "type")] pub kind: u8 }   // 0 Generic
pub struct Card { pub cardholder_name, pub brand, pub number /*Sensitive*/, pub exp_month, pub exp_year, pub code /*Sensitive*/ }
pub struct Identity { title, first_name, middle_name, last_name, address1, address2, address3, city, state,
                      postal_code, country, company, email, phone, ssn /*Sensitive*/, username,
                      passport_number /*Sensitive*/, license_number /*Sensitive*/ }
pub struct SshKey { pub private_key: Sensitive, pub public_key: String, pub key_fingerprint: String }
pub struct PasswordHistory { pub last_used_date: Option<String>, pub password: Sensitive }
pub struct Attachment { pub id: String, pub file_name: String, pub size: String, pub size_name: String, pub url: Option<String> }
pub struct Folder { pub id: Option<String>, pub name: String }
```

Value conventions for new data written by lazybw:

- Card `brand`: one of `Visa`, `Mastercard`, `Amex`, `Discover`, `Diners Club`, `JCB`, `Maestro`,
  `UnionPay`, `RuPay`, `Other` (auto-detected from the number, editable).
- Card `expMonth`: `"1"`–`"12"` (no leading zero, like the web vault) **[verify]**; `expYear`: four digits.
  Existing values are preserved as they are.
- Identity `title`: `Mr`, `Mrs`, `Ms`, `Mx`, `Dr`.
- SSH key: `privateKey` OpenSSH PEM, `publicKey` `ssh-ed25519 AAAA… comment`, `keyFingerprint` `SHA256:…`.
- New items: `reprompt: 0`, `favorite: false`, `fields: []`, `passwordHistory: null`, `secureNote:
  {"type": 0}` for notes. Do not send `id`, `object`, dates or `collectionIds` on create.

`Sensitive` = newtype over `String` with `Zeroize + ZeroizeOnDrop`, `serde(transparent)`, `Debug` →
`Sensitive(***)`, an `expose()` accessor, and no `Display`.

Round-trip requirement: for every fixture item, `from_str → to_value` equals the original JSON after
removing keys whose value is `null` on both sides. Add a `proptest` that inserts random unknown keys at
random depths and checks they survive.

### Nested folders

Bitwarden nests folders by name: `Work/Projects/2026` is shown under `Work` → `Projects`. A parent that
does not exist as a folder is a **virtual node** (filter = union of its children; cannot be a move
target; rename/delete act on real folders only). Renaming a real folder offers "Also rename N subfolders"
(default on), which renames every folder whose name starts with `old/`. Deleting offers "Also delete N
subfolders" (default off). Each folder change is one PUT/DELETE; the batch is one undo step.

## 8. Subprocess commands

All return `Result<T, BwError>`; all log the command name and exit code (never env values).

### 8.1 Status

`bw status --response --nointeraction` → `{"success":true,"data":{"object":"template","template":{...}}}`.
`serverUrl` is `null` for the default US cloud.

### 8.2 Interactive login (TUI suspended)

1. Leave the alternate screen, disable raw mode, show the cursor, print:
   `lazybw › Running "bw login". Follow the prompts; lazybw resumes when it finishes.`
2. Run `bw login --raw` (or `bw login --sso --raw`) with **stdin and stderr inherited** and **stdout
   piped**. The CLI writes every prompt (email, master password, two-step method, two-step code, new
   device code) to stderr, so the user sees them; with `--raw`, stdout receives only the session key.
3. Exit 0 → session key = trimmed stdout (`SecretString`). Non-zero → print
   `lazybw › Login failed. Press Enter to return.` and wait for Enter, so the error stays readable.
4. Restore the TUI and redraw. On success go to startup step 5 with the session key (no second
   password prompt).

### 8.3 API key login

Form fields `client_id`, `client_secret` (masked). Run `bw login --apikey --response --nointeraction` with
`BW_CLIENTID` and `BW_CLIENTSECRET` in the child env. Success leaves the vault **locked** → Unlock screen.

### 8.4 Logout and server configuration

- Logout: stop serve (§12) → `bw logout --response --nointeraction` → Login screen.
- Server: only while unauthenticated. `bw config server <url> --response --nointeraction` with
  `https://vault.bitwarden.com` (US), `https://vault.bitwarden.eu` (EU) or the user's URL. The CLI answers
  "Logout required before server config update." when logged in.

### 8.5 Export

Destination files are written by lazybw: create `<dest>.lazybw-tmp` with mode 0600 in the destination
directory, write, `fsync`, rename. Default name: `bitwarden_export_YYYYMMDDHHMMSS.<ext>` in `$HOME`.

| Format | Command (env `BW_SESSION`) | Notes |
|---|---|---|
| JSON | `bw export --format json --raw --nointeraction` | stdout = file content |
| CSV | `bw export --format csv --raw --nointeraction` | only logins and secure notes |
| JSON encrypted with account key | `bw export --format encrypted_json --raw --nointeraction` | importable only into the same account |
| JSON encrypted with a password | PTY: `bw export --format encrypted_json --password --output <tmp path>` | see below |

Password-protected export through a PTY (the password must not appear in argv):

1. Spawn with `portable-pty` (no `--nointeraction`, no `--raw`), env `BW_SESSION`.
2. Read output until it matches `(?i)password` (prompt is "Export file password:"), timeout 30 s.
3. Write the password followed by `\r`; zeroize the buffer.
4. Wait for exit (timeout 120 s). Verify the tmp file exists, parses as JSON with `"encrypted": true`
   and `"passwordProtected": true`, chmod 0600, rename to destination.
5. Never log PTY output after the password was sent.

### 8.6 Import

`bw import <format> <path> --response --nointeraction` with `BW_SESSION`; for
`bitwardenpasswordprotected` add `--passwordenv LAZYBW_IMPORT_PASSWORD` and that variable in the child env.
Success message: `Imported N item(s) from <path>` (parse `N` loosely; fall back to "Import finished").
Afterwards `POST /sync`. While the import runs, the `BwService` queue is paused.

Formats list: `bw import --formats` (requires login). Capture its real output, add it as a
fixture, and parse it; also ship a built-in fallback list. Featured at the top of the picker:
`bitwardenjson`, `bitwardenpasswordprotected`, `bitwardencsv`, then the rest alphabetically
(`1password1pux`, `chromecsv`, `firefoxcsv`, `keepass2xml`, `lastpasscsv`, …).

Duplicate-skipping (Bitwarden JSON only): write a filtered copy without the selected duplicates to
`$XDG_RUNTIME_DIR/lazybw/import-<random>.json` (0600), import that, then overwrite and delete it.

### 8.7 Password health report

`bw report password-health --response --nointeraction [--no-check-exposed]` with `BW_SESSION`
(capability `password_health`; `--no-check-exposed` when `health.check_exposed = false`).
`data` is a list response (verified against the bundled CLI source of
2026.10.1): `{"object": "list", "data": [ … ]}`. Each row is:

```json
{ "object": "password-health", "id": "…", "name": "…", "passwordStrength": 0,
  "reuseCount": 3, "exposed": true, "exposedCount": 3812, "exposedError": null }
```

lazybw also accepts a bare array for older builds and the test double.

`reuseCount` is `null` unless > 1. `exposedError` is set when the HIBP lookup failed.

## 9. Error mapping

| `message` from bw (exact, case-insensitive) | `BwError` | lazybw reaction |
|---|---|---|
| `Vault is locked.` | `Locked` | zeroize, Unlock screen |
| `You are not logged in.` | `NotLoggedIn` | stop serve, Login screen |
| `Invalid master password.` | `InvalidPassword` | shake + message on Unlock / re-prompt |
| `Not found.` | `NotFound` | toast "Item no longer exists. Syncing…", sync |
| `Premium status is required to use this feature.` | `PremiumRequired` | toast; indicates a lazybw bug |
| `Logout required before server config update.` | `LogoutRequired` | message on Login screen |
| `More than one result was found…` | `Ambiguous` | lazybw bug (always use ids) |
| other | `Other(message)` | statusline + `:messages` |
| transport error / child exited | `Disconnected` | restart serve (§12) |
| timeout | `Timeout { endpoint }` | toast, keep state |
| HTTP 403 | `Rejected` | internal error screen with log path |

## 10. Session key and master password re-prompt

- Sources: stdout of `bw login --raw`, or `data.raw` of `POST /unlock`.
- Stored in `BwService` as `SecretString`; given to subprocesses as `BW_SESSION`; given to a new serve
  child after a crash restart.
- "After unlocking, any previous session keys will no longer be valid" (CLI help): replace the stored key
  after **every** successful `/unlock`.
- Re-prompt (`reprompt = 1`) and export confirmation both call `POST /unlock` with the typed master
  password while unlocked. Success → action allowed and session key replaced. **[verify]** that unlocking
  an already unlocked serve works and keeps the vault usable.
- `security.reprompt_grace` (default `0s`) allows skipping repeated prompts within a window.

## 11. Sync and cache consistency

- Mutations update the cache from the response body (create/edit) or by `GET /object/item/{id}` (trash,
  restore) or by removal (permanent delete). Folder changes re-fetch the folder list.
- Background sync every `vault.sync_interval`: `POST /sync`, then reload (§6.3); keep selection by id,
  scroll offset, filters, marks and search.
- If an item open in the editor changed remotely (newer `revisionDate` after sync), show a banner:
  "This item changed on the server. :w overwrites it, :e! reloads it."
- Undo entries referencing items that no longer exist are dropped with a toast.

## 12. Shutdown and crash handling

- Quit: cancel pending requests → drop `SendRequest` and the stream (serve stops itself) → wait 2 s →
  `SIGTERM` → wait 3 s → `SIGKILL`. Clear the clipboard if pending. Restore the terminal.
- Child exit while running: `Disconnected` → toast "Bitwarden CLI stopped. Restarting…" → respawn with the
  stored session key → `GET /status` → `unlocked` ⇒ reload; `locked` ⇒ Unlock screen. Two failures within
  60 s ⇒ error screen with the last 20 stderr lines (redacted) and the log path.

## 13. `fake-bw` (test double)

A Rust binary in `crates/fake-bw` that behaves like the subset of `bw` lazybw uses. Tests point lazybw to
it with `LAZYBW_BW=<path>`.

State: JSON file at `$FAKE_BW_STATE`:

```json
{ "version": "2026.10.1", "status": "locked", "serverUrl": null, "email": "you@example.com",
  "masterPassword": "correct horse", "sessionKey": "fake-session",
  "folders": [ { "id": "…", "name": "Personal" } ],
  "items": [ /* Item JSON as returned by bw, including deletedDate for trash */ ],
  "log": [ /* appended by fake-bw: { "cmd": [...argv], "env": ["BW_SESSION?"], "request": {...} } */ ] }
```

Behaviour:

- `--version`, `status --response`, `login [--raw|--sso|--apikey]` (prompts on stderr: `? Email address:`,
  `? Master password: [hidden]`, `? Two-step login code:` when `"twoFactor": true` in state),
  `logout`, `config server`, `export` (all formats; `--password` prompts `? Export file password:`),
  `import` (bitwardenjson, bitwardenpasswordprotected; `--formats`), `report password-health`.
- `serve --hostname fd+connected://N | unix://PATH`: implements every endpoint in §6.2 with the exact
  envelopes, status codes, the Host/Origin checks, `"Vault is locked."` when locked, soft/permanent
  delete, restore, folder deletion moving items to no folder, password history on password change (max 5),
  `revisionDate` updates. Exits when the connected socket closes.
- Records argv and the *names* of env vars of each invocation in `log` so tests can assert that no secret
  is passed on argv.
- Fault injection: `FAKE_BW_LATENCY_MS`, `FAKE_BW_FAIL="unlock:Invalid master password.,sync"`,
  `FAKE_BW_CRASH_AFTER=<n requests>`.
- `fake-bw gen-fixture --items 5000 > big.json` for performance tests.
