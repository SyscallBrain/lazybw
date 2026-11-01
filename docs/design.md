# lazybw — Visual Design Specification

The colour reference is `docs/mockups.html` (open it in a browser; it switches between all built-in
themes). This document is the authoritative spec: layout, components, every screen, icons, themes,
motion and copy. Mockups below are drawn with Unicode icons; the app uses Nerd Font icons by default
(§5). Mockup widths are illustrative; the proportions in §2 are what you implement.

---

## 1. Principles

1. **Keyboard first, Vim always.** Everything works without the mouse; the mouse is a bonus.
2. **Secrets stay hidden.** Revealing is explicit (`za`) and temporary. Copying never reveals.
3. **One colour, one meaning.** Colours come from semantic tokens (§6). Each item type has its own colour
   in every theme. Red means danger, orange means warning, nothing else uses them.
4. **Hints where the cursor is.** Key hints appear like Neovim inlay hints, only on the active row.
5. **The UI never freezes.** Every `bw` call is asynchronous with a spinner and a result message.
6. **Fits any terminal.** Three panes from 140 columns, down to a single pane at 80×24.
7. **Quiet chrome, loud content.** Borders and labels are muted; names, values and the focused pane carry
   the colour.

## 2. Layout system

### 2.1 Screen regions (top to bottom)

| Region | Height | Content |
|---|---|---|
| Topbar | 1 row, `bg.bar` | logo, tabs, account, server, vault state |
| Body | remaining | panes or a full-screen view |
| Statusline | 1 row, `bg.bar` | mode pill, breadcrumb, showcmd, indicators, position pill |
| Message line | 1 row, `bg` | messages, `:` command line, `-- INSERT --` |

### 2.2 Pane anatomy

```
╭ Title 198 ─────────────────────── right info ╮   rounded border (BorderType::Rounded)
│▌● GitHub                            ★ ◷      │   1-col gutter for the selection bar, 1-col padding
│   dev-example                                ┃   scrollbar drawn on the right border when content overflows
╰──────────────────────────────────────────────╯
```

- Title: ` Title ` at top-left, optional count in `fg.subtle`; right title ` info ` at top-right in
  `fg.muted`.
- Focused pane: border `border.focus`, title `accent` bold. Unfocused: border `border`, title `fg.muted`.
- Panes touch each other (no gap), like lazygit.
- Section labels inside panes: UPPERCASE, `fg.muted`, bold, one blank row above (except the first).
- Selection in a focused list: `selection` background on all rows of the item + `▌` in `accent` in the
  gutter + name bold. In an unfocused list: `bg.highlight` background, no bar.

### 2.3 Breakpoints

| Width | Layout |
|---|---|
| ≥ 140 cols | Sidebar 26 cols · List `clamp(36, 40% of rest, 56)` · Detail = rest (≥ 48) |
| 100–139 | Sidebar becomes a 5-col **rail** of icons (active filter highlighted); `<leader>b` opens the full sidebar as a left overlay. List `clamp(34, 42% of rest, 48)` · Detail = rest |
| < 100 | One pane at a time. `l`/`Enter` goes deeper (sidebar → list → detail), `h` goes back. The breadcrumb in the statusline shows where you are. |
| Height < 24 | Compact list density is forced; the topbar merges into the statusline (tabs shown as `[1]2 3`). |
| < 60 cols or < 16 rows | "Terminal too small" screen: `lazybw needs at least 60×16. Current: 52×14.` |

Popups: small 40–56 cols, medium 60–80 cols, large 80% × 80% (max 120 × 40), quick open 84% × 70%.
Below 60 cols popups become full screen.

### 2.4 Density

- **Comfortable** (default): two rows per item — name with indicators, then the subtitle (username,
  `•••• 4242`, full name, first line of the note, key fingerprint) in `fg.muted`.
- **Compact**: one row — `● GitHub  dev-example                 ★ ◷`, subtitle in `fg.muted` after two
  spaces, truncated with `…`.

## 3. Components

| Component | Spec |
|---|---|
| **Topbar** | `◆ lazybw` (diamond `type.note`/magenta, name `accent` bold), then tabs ` 1 Vault `, ` 2 Generator `, ` 3 Health `. Active tab: `selection` bg, `fg` bold, digit `accent`, underline `accent` (use `Modifier::UNDERLINED` on the label). Right side: email `fg.subtle` │ server host `fg.subtle` │ state (`● unlocked` `ok`, `● syncing` `info` with spinner, `● locked` `error`). Separators `│` in `border`. |
| **Statusline** | Left: mode pill (` NORMAL ` etc., text `bg.bar` bold on `mode.*`), powerline separator, breadcrumb segment (`bg.highlight`, `fg.alt`) with its own separator. Middle: flexible space, then showcmd (pending keys and count) in `caution` bold. Right segments separated by `│` in `bg.highlight`: clipboard countdown (`caution`, only while pending), last sync (`⟳ 2m ago`, spinner while syncing), auto-lock time (`⚿ locks 14:32`), then the position pill (`3/198`, same colour as the mode). Separator style from `ui.statusline_separator`. |
| **Message line** | Messages in `fg.alt` (info), `warn`, `error`; `-- INSERT --` / `-- VISUAL --` in bold when relevant; becomes the `:` command line. Messages expire after 5 s (errors stay until the next key). |
| **Toasts** | Top-right, below the topbar, width ≤ 48, max 3 stacked. Rounded border coloured by level (`ok`, `info`, `warn`, `error`), icon + one or two lines. Auto-dismiss: 3 s (errors 6 s). Every toast is also written to `:messages`. |
| **Popup** | Rounded border `border.focus`, title in border, background `bg.float`. A drop shadow: one column to the right and one row below painted with the `shadow` colour. Everything behind is **dimmed**: each cell's fg and bg blended 55% toward `bg.bar`. |
| **Which-key** | Popup anchored to the bottom, full width minus 2, 3–4 columns of `key description`; keys `accent` bold, descriptions `fg`, groups `type.note` (magenta) with a `+` prefix. Title shows the pending prefix (` Space `, ` Space f `). |
| **Wildmenu** | Popup above the command line, left-aligned with the cursor column, width = longest entry + 4, max 10 rows. Selected entry `selection` + bold; the typed prefix in `match`. Right-aligned notes in `fg.muted` (e.g. `default`, `light`). |
| **Spinner** | Braille frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` at 80 ms, colour `info`. |
| **Strength meter** | 10 cells `▰` filled / `▱` empty. Filled colour by score: 0 `error`, 1 `warn`, 2 `caution`, 3–4 `ok`. Label after it in `fg.muted`: `very weak`, `weak`, `fair`, `strong`, `very strong`, plus context (`· changed 4 months ago` / `· 20 characters`). |
| **TOTP** | Code in `totp.code` bold, digits grouped `482 913` (6), `4829 1374` (8), Steam 5 chars ungrouped. Under it a bar of `━` (filled `totp.fresh`, empty `border`) whose length = remaining/period; colour switches to `totp.expiring` under 10 s and `totp.critical` under 5 s; seconds right-aligned in the same colour. |
| **Password text** | When revealed or in the generator/editor: letters `fg`, digits `pw.digit`, symbols `pw.symbol`. Hidden: `•` × min(len, 20) in `fg.alt` (never reveals length beyond 20). |
| **Inline hints** | Right-aligned on the active row in `fg.muted`, keys in `hint.key` (e.g. `za reveal · yp`). When the list is focused, the detail pane shows hints for `yu`, `yp`, `yt` on their fields. Toggle with `ui.show_hints`. |
| **Scrollbar** | ratatui `Scrollbar` on the right border: track `border`, thumb `accent` (focused) or `fg.subtle`. |
| **Empty states** | Centred two lines: what is empty (`fg`) and how to fill it (`fg.muted` with keys): "No items in Work" / "Press a to add one, or m on an item to move it here." |
| **Checkbox / radio** | `☑`/`☐` and `◉`/`○`. Checked in `ok` / `accent`; unchecked in `fg.subtle`. |
| **Option chip** (enum values in editor) | `‹ Base domain ›` in `type.note` (magenta); `Ctrl-a`/`Ctrl-x` cycles. |
| **Type chip** (custom field type) | `hidden`, `boolean`, `text`, `linked` in a 1-row bordered box (`border`, `fg.muted`). |
| **Dirty flag** | `[+]` in `warn` after the title of a modified editor or keymap editor. |

## 4. Screens

### 4.1 Starting

```
                                   ╻  ┏━┓╺━┓╻ ╻┏┓ ╻ ╻
                                ◆  ┃  ┣━┫┏━┛┗┳┛┣┻┓┃╻┃
                                   ┗━╸╹ ╹┗━╸ ╹ ┗━┛┗┻┛

                               ⠹ Starting Bitwarden CLI…
                               bw 2026.10.1 · /usr/bin/bw
```
Logo in `accent`, diamond in `type.note`. Shown at most until `bw serve` answers.

### 4.2 Error screens

```
╭ Bitwarden CLI not found ─────────────────────────────────────────╮
│ lazybw needs the official Bitwarden CLI (bw) 2025.9.0 or newer.  │
│                                                                  │
│ Install it with one of:                                          │
│   npm install -g @bitwarden/cli                                  │
│   your distribution's package (bitwarden-cli)                    │
│                                                                  │
│ Searched: bw.path (unset) · $LAZYBW_BW (unset) · $PATH           │
│                                                                  │
│ r retry   q quit                                                 │
╰──────────────────────────────────────────────────────────────────╯
```
Variants: **bw too old** ("Found bw 2025.6.0. Update it with `npm update -g @bitwarden/cli`."),
**config error** (path, line and column, message, `e` opens the config in `$EDITOR`, `r` retry),
**serve keeps failing** (last 20 redacted stderr lines, log path).

### 4.3 Login

```
                                   ╻  ┏━┓╺━┓╻ ╻┏┓ ╻ ╻
                                ◆  ┃  ┣━┫┏━┛┗┳┛┣┻┓┃╻┃
                                   ┗━╸╹ ╹┗━╸ ╹ ┗━┛┗┻┛
                                 Log in to Bitwarden

                ╭ Server ───────────────────────────────────────────╮
                │▌ bitwarden.com                   United States    │
                │  bitwarden.eu                    European Union   │
                │  Self-hosted…                    your server URL  │
                ╰───────────────────────────────────────────────────╯
                ╭ Method ───────────────────────────────────────────╮
                │▌ Email and password              two-step login   │
                │  Single sign-on                  opens a browser  │
                │  API key                         id and secret    │
                ╰───────────────────────────────────────────────────╯
                 Enter continue · j k move · Tab switch box · q quit
```

- "Self-hosted…" opens a one-line input (`https://vault.example.com`).
- "Email and password" / "Single sign-on" suspend the TUI (bw-integration.md §8.2); the terminal shows:

```
lazybw › Running "bw login". Follow the prompts; lazybw resumes when it finishes.
? Email address: you@example.com
? Master password: [hidden]
? Two-step login code: 123456
```

- "API key" shows a form popup: `client_id`, `client_secret` (masked), `Enter` logs in, then Unlock.

### 4.4 Unlock

```
                                   ╻  ┏━┓╺━┓╻ ╻┏┓ ╻ ╻
                                ◆  ┃  ┣━┫┏━┛┗┳┛┣┻┓┃╻┃
                                   ┗━╸╹ ╹┗━╸ ╹ ┗━┛┗┻┛

                                    Vault locked
                        you@example.com · vault.bitwarden.eu
                     ╭ Master password ─────────────────────────╮
                     │ ●●●●●●●●●●●●●▏                           │
                     ╰──────────────────────────────────────────╯
                     locked after 15 min idle · last sync 2h ago

                    Enter unlock    Ctrl-o log out    Ctrl-c quit
```

- Mask with `●` per character, beam cursor. While unlocking: `⠹ Unlocking…` replaces the hint line.
- Wrong password: shake animation (§7), border `error`, message `Invalid master password.` in `error`
  under the box; the field keeps its content selected (typing replaces it).

### 4.5 Vault

**Wide (≥ 140 cols)**

```
 ◆ lazybw   1 Vault   2 Generator   3 Health                              you@example.com │ vault.bitwarden.eu │ ● unlocked
╭ Filters ──────────────╮╭ Logins 198 ─────────────────── name ↓ ╮╭ GitHub ───────────────────────────────────────── Login ╮
│  ≡ All items      247 ││ ● Amazon                              ││ ● GitHub                                    ★ favorite │
│  ★ Favorites       12 ││   shop@example.com                    ││ Login · Personal · edited 3 days ago                   │
│ TYPES                 ││ ● Cloudflare                       ◷  ││ USERNAME                                               │
│▌ ● Logins         198 ││   ops@example.com                     ││ dev-example                                         yu │
│  ■ Cards            4 ││▌● GitHub                         ★ ◷  ┃│ PASSWORD                                               │
│  ▲ Identities       3 ││▌  dev-example                         ┃│ ••••••••••••••••••••                    za reveal · yp │
│  ◆ Secure notes    38 ││ ● GitLab                           ◷  ┃│ ▰▰▰▰▰▰▰▰▱▱ strong · changed 4 months ago               │
│  ◇ SSH keys         4 ││   dev@example.com                     ││ VERIFICATION CODE                                      │
│ FOLDERS               ││ ● Gmail                          ★ ◷  ││ 482 913                                             yt │
│  ▾ Personal        80 ││   personal@example.com                ││ ━━━━━━━━━━━━━━━━━━━━━━━━━━──────────────           18s │
│    ├ Banking        6 ││ ● Hetzner Cloud                    ⚠  ││ WEBSITES                                               │
│    └ Health         4 ││   admin@example.com                   ││ https://github.com                    base domain · gx │
│  ▸ Work            60 ││ ● Netflix                             ││ https://gist.github.com                           host │
│  ▸ Servers         21 ││   home@example.com                    ││ CUSTOM FIELDS                                          │
│  · No folder       20 ││ ● Proton Mail                    ★ ◷  ││ Recovery token    ••••••••••                    hidden │
│ ───────────────────── ││   me@example.com                      ││ 2FA enabled       ☑ yes                        boolean │
│  ✕ Trash            5 ││ ● Reddit                           ⚠  ││ NOTES                                                  │
│                       ││   u/example                           ││ Recovery codes are in the safe, drawer 2.              │
│                       ││                                       ││ ────────────────────────────────────────────────────── │
│                       ││                                       ││ created 12 Mar 2023            password history (3) gh │
╰───────────────────────╯╰───────────────────────────────────────╯╰────────────────────────────────────────────────────────╯
 NORMAL  Vault › Logins › GitHub                                         ⧗ clipboard 23s │ ⟳ 2m ago │ ⚿ locks 14:32   3/198
 Copied password for “GitHub”. Clipboard clears in 30s.
```

- Sidebar entries: All items, Favorites, TYPES (5 types; hide a type with 0 items unless
  `vault.show_empty_types`), FOLDERS (tree, virtual parents in `fg.muted`), No folder, separator,
  Archive (only when archived items exist, read-only), Trash. Counts right-aligned in `fg.subtle`.
- List indicators (right-aligned, in this order): `★` fav (`fav`), `◷` TOTP (`info`), passkey icon
  (`fg.alt`), attachment count (`fg.subtle`, read-only), `⚠` health issue (`error` if exposed, `warn`
  otherwise; only after a health run), `⚿` reprompt (`fg.subtle`).
- Row subtitle per type: login → username (or first URI host); card → `Visa •••• 4242`; identity →
  full name; note → first non-empty line of the note (secret-safe: notes are shown in lists only if
  `reprompt = 0`); SSH key → fingerprint shortened `SHA256:Qx9…7fA`.
- Trash filter: list title ` Trash 5 `, subtitle shows `deleted 3 days ago`; detail is read-only with
  a banner `In trash · r restore · D delete permanently`.

**Medium (100–139 cols)** — the sidebar becomes a rail:

```
╭───╮╭ Logins 198 ───────────── name ↓ ╮╭ GitHub ───────────────────── Login ╮
│ ≡ ││ ● Amazon                        ││ ● GitHub                ★ favorite │
│ ★ ││   shop@example.com              ││ Login · Personal · edited 3 days…  │
│ ─ ││▌● GitHub                   ★ ◷  ││ USERNAME                           │
│▌● ││▌  dev-example                   ││ dev-example                     yu │
│ ■ ││ ● GitLab                     ◷  ││ …                                  │
│ ▲ ││                                 ││                                    │
│ ◆ ││                                 ││                                    │
│ ◇ ││                                 ││                                    │
│ ─ ││                                 ││                                    │
│ ▾ ││                                 ││                                    │
│ ✕ ││                                 ││                                    │
╰───╯╰─────────────────────────────────╯╰────────────────────────────────────╯
```
The folder icon in the rail opens the folder picker (`<leader>ff`).

**Narrow (< 100 cols)** — one pane, breadcrumb in the statusline (`Vault › Logins`), `l` opens the
detail full width, `h` returns.

### 4.6 Detail views

Common header: icon + name (bold, `fg`), favourite on the right; meta line `Type · Folder · edited
<relative>` in `fg.muted`. Fields are rows; labels per §2.2; footer with created date and shortcuts.
If `reprompt = 1`, secret fields show `•••••• (master password required)` and the first reveal/copy
opens the re-prompt dialog (§4.14).

**Login** — as in §4.5. Passkeys appear as a field `PASSKEY` → `github.com · dev-example · created
12 Mar 2025` (read-only; `dd` on it asks to remove it). Password history: `gh` popup (§4.14).

**Card**

```
│ ■ Personal Visa                                 ★ │
│ Card · Banking · edited 2 months ago              │
│ CARDHOLDER NAME                                   │
│ Jane Example                                      │
│ NUMBER                                            │
│ •••• •••• •••• 4242                     Visa   yy │
│ EXPIRATION              SECURITY CODE             │
│ 04 / 2028               •••                    za │
│ NOTES                                             │
│ Contactless limit raised in 2025.                 │
```
Revealed numbers are grouped by 4 (Amex 4-6-5). Expired cards show the date in `warn` with `expired`.

**Identity** — sections NAME (title, first, middle, last), CONTACT (email, phone, username, company),
ADDRESS (address 1–3, city, state, postal code, country — rendered as a postal block), IDENTIFIERS (SSN,
passport number, licence number — hidden by default like passwords).

**Secure note** — the note body fills the pane, soft-wrapped, plain text (no Markdown rendering), with
relative line numbers in `fg.subtle` when focused (`j`/`k` move by line, `yy` copies the line, `yn`
copies the whole note).

**SSH key**

```
│ ◇ Hetzner deploy key                              │
│ SSH key · Servers · edited 1 year ago             │
│ PUBLIC KEY                                        │
│ ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA…Qx9 deploy   │
│ FINGERPRINT                                       │
│ SHA256:Qx9hY2…f7A                                 │
│ PRIVATE KEY                                       │
│ -----BEGIN OPENSSH PRIVATE KEY----- ••••    za    │
```
`yy` on the public key copies it whole; the private key reveals as a scrollable block.

**Unknown type** (6, 7, 8, future) — banner `This item type (Passport) is shown read-only.` then every
string field of the type object as `LABEL` / value rows (labels from camelCase → "Passport number"),
values hidden if the key contains `number`, `code`, `pin`, `ssn`, `iban`, `account` or `password`.

### 4.7 Editor

```
 ◆ lazybw   1 Vault   2 Generator   3 Health                                                         ● unlocked
╭ Edit login · GitHub [+] ───────────────────────────────────────────────────────────────── :w save · :q close ╮
│ CREDENTIALS                                            │ WEBSITES                          o new · dd remove │
│  Name                  GitHub                          │  1  https://github.com              ‹ Base domain › │
│  Folder                Personal ▾               Ctrl-f │  2  https://gist.github.com                ‹ Host › │
│  Username              dev-example                     │ CUSTOM FIELDS                                       │
│▌ Password              k7#Qm!vR2x@Lp9$Wn4eT▏    Ctrl-g │  Recovery token   [hidden]   ••••••••••             │
│                        ▰▰▰▰▰▰▰▰▰▰ very strong          │  2FA enabled      [boolean]  ☑                      │
│  Authenticator key     ••••••••••••••••      482 913 ✓ │ NOTES                                Ctrl-e $EDITOR │
│ OPTIONS                                                │   1 Recovery codes are in the safe, drawer 2.       │
│  Favorite              ☑                               │   2 Account created with the old email.             │
│  Master password ask   ☐                               │   ~                                                 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 INSERT  Edit › GitHub › Password   [+] unsaved                                                 ✓ valid    4/12
 -- INSERT --   Esc normal · Tab next field · Ctrl-a / Ctrl-x change option
```

- Two columns at ≥ 120 cols (left: credentials and options; right: lists and notes); one column below.
- The editor replaces list + detail (the sidebar stays if visible). Title: `Edit <type> · <name>` or
  `New <type>`; `[+]` when dirty.
- Field row: label column 22 wide (`fg.alt`; active label `mode.insert` bold in INSERT, `accent` in
  NORMAL), value, right-aligned hint. Active row: `bg.highlight` + bar in the mode colour.
- Password fields show the password in clear while the editor is open **only after** `za`; by default
  they are masked even in the editor; the generator inserts and reveals.
- Validation inline under the field in `error` (e.g. `Not a valid URL`, `Card number fails the Luhn
  check` as `warn`, `Not a valid authenticator key`). Saving with errors is refused with a message.
- Authenticator key field shows a live code preview `482 913 ✓` when valid.
- Fields per type:
  - Login: Name, Folder, Username, Password, Authenticator key; Websites (URI + match option);
    Custom fields; Notes; Favorite; Master password re-prompt.
  - Card: Name, Folder, Cardholder name, Number (brand detected and shown as a chip), Brand (option),
    Expiration month (option `01 - January`…), Expiration year (number; `Ctrl-a`/`Ctrl-x` ±1),
    Security code; Custom fields; Notes; options.
  - Identity: Name, Folder, Title (option), First, Middle, Last, Username, Company, Email, Phone, Address
    1–3, City, State, Postal code, Country, SSN, Passport number, Licence number; Custom fields; Notes;
    options.
  - Secure note: Name, Folder, Notes (large), Custom fields, options.
  - SSH key: Name, Folder, two actions `Generate Ed25519 key` / `Import from file…` (path input with
    completion, reads OpenSSH private keys; passphrase-protected keys prompt for the passphrase to
    decrypt and are stored decrypted, with a warning), read-only Public key and Fingerprint, Private key
    (masked); Custom fields; Notes; options.
- Custom field rows: name, type chip, value. `Enter` on the type chip opens `Text / Hidden / Boolean /
  Linked`; Linked opens the list of linkable fields for the item type (bw-integration.md §7).

**New item type picker** (`a`):

```
╭ New item ───────────────╮
│ l   ● Login             │
│ n   ◆ Secure note       │
│ c   ■ Card              │
│ i   ▲ Identity          │
│ s   ◇ SSH key           │
╰─────────────────────────╯
```

### 4.8 Quick open (`Ctrl-p`)

```
╭ Find ───────────────────────────────────────────────────────────────────────────────── 247 items ╮
│ ❯ git▏                                         5 / 247 │╭ Preview ─────────────────────────────╮ │
│ ────────────────────────────────────────────────────── ││ ● GitHub                             │ │
│▌● GitHub                                      Personal ││ USERNAME                             │ │
│ ● GitLab                                          Work ││ dev-example                          │ │
│ ● Gitea · home                                 Servers ││ PASSWORD                             │ │
│ ◆ GPG key for commits                             Work ││ ••••••••••••••••••••                 │ │
│ ● DigitalOcean                                 Servers ││ VERIFICATION CODE                    │ │
│                                                        ││ 482 913                          18s │ │
│                                                        ││ WEBSITE                              │ │
│                                                        ││ https://github.com                   │ │
│                                                        │╰──────────────────────────────────────╯ │
│ Enter open  Ctrl-y copy password  Ctrl-u username  Ctrl-t code  Ctrl-e edit  Esc close           │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
```
Prompt `❯` in `type.note`; matched characters in `match`; folder in `fg.muted`. Statusline mode pill
` SEARCH ` (`mode.search`). Copy actions close the finder and show the clipboard toast. Preview is hidden
below 100 cols.

### 4.9 Command line and which-key

```
                          ╭────────────────────────────╮
                          │ tokyonight-night   default │
                          │ tokyonight-storm           │   ← selected entry, previewed live
                          │ tokyonight-moon            │
                          │ tokyonight-day       light │
                          ╰────────────────────────────╯
 COMMAND  live preview
:theme tokyonight-s▏
```

```
╭ Space ───────────────────────────────────────────────────────────────────────────────────╮
│ n  new item           f  +folders           g  generator          h  vault health        │
│ /  find any item      s  +sort              t  theme              e  export              │
│ i  import             S  sync (full)        l  lock               b  sidebar             │
│ d  density            m  messages           ,  settings           k  keybindings         │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
 NORMAL  Vault › Logins                                                           ␣   3/198
```

### 4.10 Generator (tab 2, also a popup from the editor with `Ctrl-g`)

```
╭ Generator ─────────────────────────────────────────────────╮╭ This session ──────────────────────────╮
│  [ Password ]  Passphrase                              Tab ││▌Rv8%qTn!Ze3wLx&…                   now │
│ ╭────────────────────────────────────────────────────────╮ ││ anchor-marble-crisp-velvet-7     2 min │
│ │ Rv8%qTn!Ze3wLx&p6MuJ#c2yB              r new · yy copy │ ││ k7#Qm!vR2x@Lp9$Wn4eT             6 min │
│ ╰────────────────────────────────────────────────────────╯ ││ orbit-tundra-plaza-quiver        9 min │
│                ▰▰▰▰▰▰▰▰▰▰ very strong                      ││ ────────────────────────────────────── │
│ Length              24  ━━━━━━━━━━━━●──────────      5–128 ││ Kept in memory only. Cleared on lock.  │
│ Characters          ☑ A-Z  ☑ a-z  ☑ 0-9  ☑ !@#$%^&*        ││                                        │
│ Minimum numbers     2                                      ││                                        │
│ Minimum special     2                                      ││                                        │
│ Avoid ambiguous     ☐  l 1 I O 0                           ││                                        │
│ h l length · x toggle option · Enter use in item           ││                                        │
╰────────────────────────────────────────────────────────────╯╰────────────────────────────────────────╯
```
Passphrase mode options: Words (3–20), Separator (input; `space`/`empty` accepted), Capitalize,
Include number. Every option change regenerates (debounced 120 ms). Defaults come from config
(`generator.*`, §9) and the last used options are remembered for the session.

### 4.11 Export wizard

Step 1 — format and destination:

```
╭ Export vault ───────────────────────────────────────────────── 1/3 ╮
│ FORMAT                                                             │
│ ◉ JSON                                    readable, every field    │
│ ○ JSON encrypted with a password          for backups              │
│ ○ JSON encrypted with your account        this account only        │
│ ○ CSV                                     logins and notes only    │
│ DESTINATION                                                        │
│▌File   ~/bitwarden_export_20261030140000.json▏                 Tab │
│ ⚠ This file will contain every password in plain text. Keep it on  │
│   an encrypted disk and delete it when you no longer need it.      │
│                                            [ Cancel ]   [ Next › ] │
╰────────────────────────────────────────────────────────────────────╯
```
Step 2 (password-protected only): file password + confirmation, strength meter. Step 3: master password
("Confirm with your master password"). Result: toast `Exported 247 items to ~/…json` and a message line
reminder for plaintext exports. `Tab` in the path completes directories/files.

### 4.12 Import wizard

Step 1 format picker (filterable; featured on top: Bitwarden (json), Bitwarden (password-protected
json), Bitwarden (csv), then all formats alphabetically). Step 2 file path (completion; for
password-protected files a password field). Step 3 preview (Bitwarden JSON only; other formats skip to
confirmation):

```
╭ Import · Preview ───────────────────────────────────────────────────── 3/3 ╮
│ ~/Downloads/bitwarden_export_20260101.json                                 │
│ Bitwarden JSON · not encrypted                                             │
│                                                                            │
│ 248 items   ● 201 logins  ◆ 30 notes  ■ 9 cards  ▲ 4 identities  ◇ 4 SSH   │
│ 6 folders   2 new: Travel, Work/Clients                                    │
│                                                                            │
│ POSSIBLE DUPLICATES (3)                                   x toggle · a all │
│▌☑ ● GitHub           dev-example            github.com                     │
│ ☑ ● Gmail            personal@example.com   google.com                     │
│ ☐ ◆ Router notes                                                           │
│                                                                            │
│ ☑ Skip checked duplicates                                                  │
│                                            [ Back ]   [ Import 246 items ] │
╰────────────────────────────────────────────────────────────────────────────╯
```
Duplicate rule: same type and same normalised name (trimmed, case-folded) and, for logins, same
username (case-folded) and same registrable domain of the first URI; for notes, same name and same note
text hash; for cards, same last 4 digits; for identities, same full name and email. After import: sync,
toast `Imported 246 items. Skipped 2 duplicates.`, sidebar switches to All items sorted by created
(newest first).

### 4.13 Health (tab 3)

```
╭ Vault health ──────────────────────────────────────────────────────────────── 198 logins checked ╮
│ 72 / 100  ▰▰▰▰▰▰▰▱▱▱   would be 88 after fixing the exposed and reused passwords                 │
│ ╭────────────╮╭────────────╮╭────────────╮╭────────────╮╭────────────╮                           │
│ │ 2          ││ 9          ││ 14         ││ 41         ││ 3          │                           │
│ │ exposed    ││ reused     ││ weak       ││ over 1 year││ no HTTPS   │                           │
│ ╰────────────╯╰────────────╯╰────────────╯╰────────────╯╰────────────╯                           │
│ EXPOSED · MOST SEVERE FIRST                                                                      │
│▌● exposed     Reddit          seen 3,812 times in data breaches                  gp new password │
│ ● exposed     Old forum       seen 112 times                                                     │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
 NORMAL  Health › Exposed                                               HIBP via k-anonymity    1/2
```
- Cards: number bold in the category colour (exposed `error`, reused `warn`, weak `caution`, old and
  insecure `fg.alt`); the selected category card has its border in the category colour and a tinted
  background; `Tab` cycles.
- **Score**: per login with a password, penalty = min(1, Σ weights) with exposed 1.0, reused 0.6,
  weak 0.5 (strength ≤ 1) or 0.25 (strength 2), old 0.15 (`health.old_password_age`, default 365 days,
  from `passwordRevisionDate` or `creationDate`), insecure 0.2 (any `http://` URI).
  `score = round(100 × (1 − Σ penalties / count))`. "Would be N" recomputes without exposed and reused.
  Score colour: ≥ 80 `ok`, 50–79 `caution`, < 50 `error`.
- Before the first run: an empty state "Check your passwords for breaches, reuse and weakness." with
  `r run check`. Running shows a spinner and "Checking 198 logins… (this contacts Have I Been Pwned
  with partial hashes only)" when `health.check_exposed` is on.
- On bw < 2026.8.0: exposure is unavailable (card shows `—` and a note); weak and reused are computed
  locally with zxcvbn and hashing.

### 4.14 Dialogs and small popups

```
╭ Delete permanently? ───────────────────────────╮   ╭ Empty trash ──────────────────────────────────╮
│ “Old forum” will be deleted for good. This     │   │ 5 items will be deleted for good. This cannot │
│ cannot be undone.                              │   │ be undone. Type empty to confirm.             │
│                     [ Cancel ]   [ Delete ]    │   │ ▏                                             │
╰────────────────────────────────────────────────╯   ╰───────────────────────────────────────────────╯
```
- Delete folder: "Delete folder “Work”? Its 60 items move to No folder." + checkbox "Also delete 3
  subfolders" (default off).
- Rename folder: input + checkbox "Also rename 3 subfolders" (default on).
- Unsaved changes on close: "Save changes to “GitHub”?  y save · n discard · c cancel".
- Master password re-prompt: `╭ Master password required ╮` "“Bank” asks for your master password." +
  masked input.
- Folder picker (`m`, `<leader>ff`): filterable tree, first entry `+ Create folder…`, current folder
  marked `●`.
- Password history (`gh`): list of `•••••••••• · used until 3 Jan 2026`; `za` reveals, `yy` copies.
- Item info (`K`): created, edited, password changed, id (copyable with `yy`), folder, type, reprompt.

### 4.15 Settings (`<leader>,`, `:set`)

```
╭ Settings ────────────────────────────────────────────────────────────────────────────────╮
│ APPEARANCE                                                                               │
│▌ Theme                       ‹ tokyonight-night ›                             Enter pick │
│  Transparent background      ☐                                                           │
│  Icons                       ‹ nerdfont ›                                                │
│  Animations                  ☑                                                           │
│  List density                ‹ comfortable ›                                             │
│  Statusline separators       ‹ arrow ›                                                   │
│  Key hints                   ☑                                                           │
│ SECURITY                                                                                 │
│  Clear clipboard after       30s                                                         │
│  Lock after inactivity       15m                                                         │
│  Hide revealed fields after  20s                                                         │
│  Hide on focus lost          ☑                                                           │
│  Show TOTP codes             ☑                                                           │
│ VAULT · GENERATOR · HEALTH · EDITOR                                                      │
│  …                                                                                       │
│  Keybindings                 Edit keybindings…                                Enter open │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
 h l change · r reset · changes are saved to ~/.config/lazybw/config.toml
```
Every setting from §9 appears here except `bw.*` and `keys.*` (keys are in the keymap editor). Changes
apply live and persist immediately (toml_edit). Values different from the default show a `●` in `accent`
before the label.

### 4.16 Keymap editor (`<leader>k`, `:keymap`)

```
╭ Keybindings [+] ─────────────────────────────────────────────────── context ‹ List › Tab ╮
│ / filter▏                                                                                │
│   DESCRIPTION                      ACTION                    KEYS                        │
│   Copy main field                  item.copy primary         yy                          │
│ ● Move to trash                    item.trash                dd  <Del>                   │
│▌  Add to or remove from favorites  item.favorite             *                           │
│   Move to folder                   item.move                 m                           │
│   Edit item                        item.edit                 e  i                        │
│   …                                                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
 Enter rebind · a add key · dd remove key · r reset · R reset context · Ctrl-s save · q close
```
Capture popup:

```
╭ Rebind “Add to or remove from favorites” · List ───────────────╮
│ Press the new keys:   g  f▏                                    │
│                                                                │
│ ⚠ gf is used by “Go to this item's folder”. Replace it? y / n  │
│                                                                │
│ Enter confirm · Backspace remove last key · Esc cancel         │
│ To bind Enter, Backspace or Escape, edit config.toml.          │
╰────────────────────────────────────────────────────────────────╯
```
User-changed rows: `●` in `accent`; session-only mappings (`:map`) show a `session` chip.

### 4.17 Help and messages

`?` opens a large popup with the bindings of the current context(s) grouped by area (generated from the
registry, showing the effective keys), searchable with `/`. `:messages` lists every message and toast of
the session with time and level colour.

## 5. Icons

`ui.icons = "nerdfont" | "unicode" | "ascii"`. Nerd Font codepoints below are Font Awesome 4 glyphs in
Nerd Fonts v3. **Verify every codepoint against Nerd Fonts' `glyphnames.json` while implementing** and
keep the table in `ui/icons.rs` (with a test that all Nerd Font entries are in the Private Use Area and
distinct per role).

| Role | Nerd Font | Unicode | ASCII |
|---|---|---|---|
| Login | `nf-fa-key` U+F084 | `●` | `L` |
| Card | `nf-fa-credit_card` U+F09D | `■` | `C` |
| Identity | `nf-fa-id_card` U+F2C2 | `▲` | `I` |
| Secure note | `nf-fa-sticky_note` U+F249 | `◆` | `N` |
| SSH key | `nf-fa-terminal` U+F120 | `◇` | `K` |
| Other type | `nf-fa-question_circle` U+F059 | `?` | `?` |
| All items | `nf-fa-bars` U+F0C9 | `≡` | `=` |
| Favorite | `nf-fa-star` U+F005 | `★` | `*` |
| TOTP | `nf-fa-clock_o` U+F017 | `◷` | `T` |
| Passkey | `nf-fa-shield` U+F132 | `◈` | `P` |
| Attachment | `nf-fa-paperclip` U+F0C6 | `⊞` | `+` |
| Warning | `nf-fa-exclamation_triangle` U+F071 | `⚠` | `!` |
| Reprompt | `nf-fa-lock` U+F023 | `⚿` | `R` |
| Folder closed / open | `nf-fa-folder` U+F07B / `nf-fa-folder_open` U+F07C | `▸` / `▾` | `>` / `v` |
| Trash | `nf-fa-trash` U+F1F8 | `✕` | `x` |
| Archive | `nf-fa-archive` U+F187 | `⊟` | `a` |
| Sync | `nf-fa-refresh` U+F021 | `⟳` | `~` |
| Clipboard | `nf-fa-clipboard` U+F0EA | `⧗` | `c` |
| Lock / unlock | U+F023 / `nf-fa-unlock` U+F09C | `⚿` / `●` | `L` / `U` |
| Website | `nf-fa-globe` U+F0AC | `↗` | `>` |
| Check / cross | `nf-fa-check` U+F00C / `nf-fa-times` U+F00D | `✓` / `✗` | `v` / `x` |
| Generator tab | `nf-fa-magic` U+F0D0 | `✦` | `G` |
| Health tab | `nf-fa-heartbeat` U+F21E | `♥` | `H` |
| Settings | `nf-fa-cog` U+F013 | `⚙` | `S` |
| Keybindings | `nf-fa-keyboard_o` U+F11C | `⌨` | `K` |
| Logo diamond | `◆` in every mode | `◆` | `*` |

Powerline separators (`ui.statusline_separator`): `arrow` U+E0B0/U+E0B2, `round` U+E0B4/U+E0B6, `slant`
U+E0BC/U+E0BA, `none` (plain space). Unicode/ASCII icon modes force `none`.

Brand icons (Nerd Font mode only) replace the Login icon when the first URI's registrable domain is known:
github.com U+F09B, gitlab.com U+F296, google.com/gmail.com U+F1A0, apple.com/icloud.com U+F179,
microsoft.com/live.com/outlook.com U+F17A, amazon.* U+F270, reddit.com U+F1A1, twitter.com/x.com
U+F099, facebook.com U+F09A, linkedin.com U+F0E1, steampowered.com U+F1B6, spotify.com U+F1BC,
slack.com U+F198, dropbox.com U+F16B, paypal.com U+F1ED, stackoverflow.com U+F16C, youtube.com U+F167,
twitch.tv U+F1E8, wikipedia.org U+F266, bitbucket.org U+F171, trello.com U+F181, telegram.org U+F2C6.
Add more (Discord, Docker, AWS, Cloudflare, Netflix…) from the `nf-md-*`/`nf-dev-*` sets after checking
`glyphnames.json`. Brand icons keep the `type.login` colour.

## 6. Themes

### 6.1 Tokens

Widgets use tokens only. Each theme defines a **palette**; tokens map to palette entries (same mapping for
all themes unless a theme overrides a token).

| Token | Palette entry | Used for |
|---|---|---|
| `bg` | `bg` | main background |
| `bg.bar` | `bg_dark` | topbar, statusline |
| `bg.float` | `bg_dark` | popups, which-key, toasts |
| `bg.highlight` | `bg_highlight` | active row in editor, active sidebar filter when unfocused, statusline segments |
| `selection` | `bg_visual` | selected rows in a focused list, active tab |
| `border` | `border` | unfocused borders, separators, empty meter cells |
| `border.focus` | `accent` | focused borders and popups |
| `fg` | `fg` | main text |
| `fg.alt` | `fg_dark` | secondary text, masked values |
| `fg.muted` | `comment` | labels, subtitles, hints |
| `fg.subtle` | `dark5` | counts, timestamps |
| `accent` | `accent` | focus, titles, selection bar, primary buttons |
| `mode.normal` / `insert` / `visual` / `command` / `search` / `locked` | `blue` / `green` / `magenta` / `yellow` / `orange` / `red` | mode pills, position pill |
| `type.login` / `card` / `identity` / `note` / `ssh` / `other` | `blue` / `orange` / `green` / `magenta` / `cyan` / `fg_dark` | item icons |
| `fav` | `yellow` | favourite star |
| `ok` / `info` / `caution` / `warn` / `error` | `green` / `cyan` / `yellow` / `orange` / `red` | states and messages |
| `match` | `orange` (bold) | search highlights |
| `link` | `cyan` (underlined) | URIs |
| `hint.key` | `blue` | keys inside hints |
| `totp.code` / `totp.fresh` / `totp.expiring` / `totp.critical` | `teal` / `teal` / `yellow` / `red` | TOTP |
| `pw.digit` / `pw.symbol` | `orange` / `magenta` | password colouring |
| `dirty` | `orange` | `[+]` |
| `shadow` | `shadow` | popup shadow cells |

`backdrop` is not a colour: popups blend underlying cells 55% toward `bg_dark`.

### 6.2 Palettes

| Entry | tokyonight-night | tokyonight-storm | tokyonight-moon | tokyonight-day |
|---|---|---|---|---|
| bg | `#1a1b26` | `#24283b` | `#222436` | `#e1e2e7` |
| bg_dark | `#16161e` | `#1f2335` | `#1e2030` | `#d0d5e3` |
| bg_highlight | `#292e42` | `#292e42` | `#2f334d` | `#c4c8da` |
| bg_visual | `#283457` | `#2e3c64` | `#2d3f76` | `#b7c1e3` |
| border | `#3b4261` | `#3b4261` | `#3b4261` | `#a8aecb` |
| fg | `#c0caf5` | `#c0caf5` | `#c8d3f5` | `#3760bf` |
| fg_dark | `#a9b1d6` | `#a9b1d6` | `#828bb8` | `#6172b0` |
| comment | `#565f89` | `#565f89` | `#636da6` | `#848cb5` |
| dark5 | `#737aa2` | `#737aa2` | `#737aa2` | `#6172b0` |
| accent | `#7aa2f7` | `#7aa2f7` | `#82aaff` | `#2e7de9` |
| blue | `#7aa2f7` | `#7aa2f7` | `#82aaff` | `#2e7de9` |
| cyan | `#7dcfff` | `#7dcfff` | `#86e1fc` | `#007197` |
| magenta | `#bb9af7` | `#bb9af7` | `#c099ff` | `#9854f1` |
| purple | `#9d7cd8` | `#9d7cd8` | `#fca7ea` | `#7847bd` |
| orange | `#ff9e64` | `#ff9e64` | `#ff966c` | `#b15c00` |
| yellow | `#e0af68` | `#e0af68` | `#ffc777` | `#8c6c3e` |
| green | `#9ece6a` | `#9ece6a` | `#c3e88d` | `#587539` |
| teal | `#73daca` | `#73daca` | `#4fd6be` | `#118c74` |
| red | `#f7768e` | `#f7768e` | `#ff757f` | `#f52a65` |
| shadow | `#0f0f14` | `#16192a` | `#191a2a` | `#b4b9cc` |

| Entry | catppuccin-mocha | gruvbox-dark | nord | rose-pine | kanagawa-wave |
|---|---|---|---|---|---|
| bg | `#1e1e2e` | `#282828` | `#2e3440` | `#191724` | `#1f1f28` |
| bg_dark | `#181825` | `#1d2021` | `#272c36` | `#1f1d2e` | `#16161d` |
| bg_highlight | `#313244` | `#3c3836` | `#3b4252` | `#21202e` | `#2a2a37` |
| bg_visual | `#45475a` | `#504945` | `#434c5e` | `#403d52` | `#2d4f67` |
| border | `#45475a` | `#504945` | `#4c566a` | `#403d52` | `#54546d` |
| fg | `#cdd6f4` | `#ebdbb2` | `#eceff4` | `#e0def4` | `#dcd7ba` |
| fg_dark | `#bac2de` | `#d5c4a1` | `#d8dee9` | `#908caa` | `#c8c093` |
| comment | `#6c7086` | `#928374` | `#616e88` | `#6e6a86` | `#727169` |
| dark5 | `#9399b2` | `#a89984` | `#8892a8` | `#908caa` | `#938aa9` |
| accent | `#89b4fa` | `#83a598` | `#88c0d0` | `#c4a7e7` | `#7e9cd8` |
| blue | `#89b4fa` | `#83a598` | `#81a1c1` | `#c4a7e7` | `#7e9cd8` |
| cyan | `#89dceb` | `#8ec07c` | `#88c0d0` | `#9ccfd8` | `#7fb4ca` |
| magenta | `#cba6f7` | `#d3869b` | `#b48ead` | `#ebbcba` | `#957fb8` |
| purple | `#f5c2e7` | `#d3869b` | `#b48ead` | `#c4a7e7` | `#d27e99` |
| orange | `#fab387` | `#fe8019` | `#d08770` | `#f6c177` | `#ffa066` |
| yellow | `#f9e2af` | `#fabd2f` | `#ebcb8b` | `#f6c177` | `#e6c384` |
| green | `#a6e3a1` | `#b8bb26` | `#a3be8c` | `#9ccfd8` | `#98bb6c` |
| teal | `#94e2d5` | `#8ec07c` | `#8fbcbb` | `#3e8fb0` | `#6a9589` |
| red | `#f38ba8` | `#fb4934` | `#bf616a` | `#eb6f92` | `#ff5d62` |
| shadow | `#11111b` | `#141617` | `#20242c` | `#12101a` | `#101015` |

**terminal** theme: palette entries map to ANSI colours so the user's terminal palette applies — `bg`,
`bg_dark` → `Reset`; `bg_highlight` → index 0; `bg_visual` → index 8; `border`, `comment` → index 8;
`fg` → `Reset`; `fg_dark`, `dark5` → index 7; `accent`, `blue` → 4; `cyan` → 6; `magenta`, `purple` → 5;
`orange` → 9; `yellow` → 3; `green` → 2; `teal` → 14; `red` → 1; `shadow` → index 0; popups keep
`bg_dark` = `Reset` but draw borders.

**transparent = true**: `bg` and `bg.bar` become `Reset`; `bg.float` stays opaque so popups remain
readable; the backdrop dim only changes foreground colours.

**Truecolor fallback**: when `COLORTERM` is not `truecolor`/`24bit`, convert hex to the nearest xterm-256
index once at theme load.

### 6.3 Theme files

```toml
# ~/.config/lazybw/themes/my-night.toml
name = "my-night"
extends = "tokyonight-night"   # optional; any built-in or user theme
appearance = "dark"            # dark | light (used by the theme picker)

[palette]                      # override or add palette entries
accent = "#bb9af7"

[tokens]                       # override token mappings: palette entry, "#rrggbb", ANSI index, or "reset"
"border.focus" = "magenta"
"match" = "#ff9e64"
```
Validation: unknown tokens/entries → warning; missing entries after `extends` → error; cycles in
`extends` → error. Built-in themes live in `assets/themes/*.toml` in this same format.

## 7. Motion (tachyonfx)

All effects are visual only: input is processed immediately, and an effect is cancelled if the area it
animates changes. Disabled when `ui.animations = false`, in tests, and when `NO_COLOR` is set. The 60 Hz
frame loop runs only while an effect is active.

| Event | Effect | Duration |
|---|---|---|
| Popup opens | content fades in from `bg.float`; backdrop dim fades in | 160 ms, quad-out |
| Popup closes | fade out | 100 ms, quad-in |
| Toast appears / leaves | sweeps in from the right / fades out | 180 ms / 200 ms |
| Item moved to trash | the item's rows dissolve, then the list closes the gap | 220 ms |
| Field or item copied | copied row flashes `selection` → normal | 250 ms |
| TOTP code changes | new digits coalesce | 200 ms |
| Wrong master password | input box shakes ±1 column three times; border flashes `error` | 300 ms |
| Vault unlocked | panes sweep in from the left | 250 ms |
| Tab switch | body fades in | 120 ms |
| Theme change | none (instant) | — |

## 8. Copy

- English, sentence case, specific verbs, no apologies, no exclamation marks, no "please". Name things the
  way Bitwarden users know them: item, login, secure note, card, identity, SSH key, folder, trash,
  master password, two-step login, verification code (TOTP), passkey.
- Buttons and actions say what happens: `Delete`, `Import 246 items`, `Restore`.
- Errors say what failed and what to do: `Couldn't save “GitHub”: Vault is locked. Unlock and save again.`
- Vim-native messages keep Vim's wording: `E37: No write since last change (add ! to override)`,
  `E492: Not an editor command: foo`.
- Dates: lists use relative time (`just now`, `2 min ago`, `3 hours ago`, `yesterday`, `3 days ago`,
  `4 months ago`, `2 years ago`); metadata uses `12 Mar 2023` (or ISO `2023-03-12` with
  `ui.date_format = "iso"`). Times `14:32` (24 h).
- Numbers use thousands separators (`3,812`).

Message catalogue (exact strings):

| Event | Message |
|---|---|
| Copy | `Copied password for “GitHub”. Clipboard clears in 30s.` (field name varies) |
| Clipboard cleared | `Clipboard cleared.` |
| Trash | `Moved “GitHub” to Trash. Press u to undo.` / `Moved 3 items to Trash. Press u to undo.` |
| Restore | `Restored “GitHub”.` |
| Save / create | `Saved “GitHub”.` / `Created “GitHub”.` |
| Permanent delete | `Deleted “Old forum” permanently.` |
| Empty trash | `Emptied the trash (5 items).` |
| Undo / redo | `Undone: <description>.` / `Redone: <description>.` |
| Sync | `Synced 247 items.` / `Sync failed: <bw message>` |
| Lock | `Vault locked.` |
| Folder | `Created folder “Work”.` / `Renamed “Work” to “Job”.` / `Deleted folder “Work”. 60 items moved to No folder.` |
| Export | `Exported 247 items to ~/bitwarden_export_20261030140000.json.` |
| Import | `Imported 246 items. Skipped 2 duplicates.` |
| Health | `Checked 198 logins. 2 exposed, 9 reused, 14 weak.` |
| Serve restart | `Bitwarden CLI stopped. Restarting…` |
| Reprompt | `“Bank” asks for your master password.` |
| Config reload | `Config reloaded.` / `Config not reloaded: <error>. Keeping the previous settings.` |
| Remote change | `This item changed on the server. :w overwrites it, :e! reloads it.` |

## 9. Configuration (default `assets/default-config.toml`)

```toml
# lazybw configuration
# Location: $XDG_CONFIG_HOME/lazybw/config.toml (usually ~/.config/lazybw/config.toml)
# Durations: "500ms", "30s", "15m", "2h", "365d", or "never" where allowed.

[bw]
path = ""                       # path to the Bitwarden CLI; empty = $LAZYBW_BW, then $PATH
transport = "fd"                # "fd" (private socket, recommended) or "unix" (socket in $XDG_RUNTIME_DIR)
inherit_session = false         # reuse BW_SESSION from the environment if it is set

[ui]
theme = "tokyonight-night"
transparent = false
icons = "nerdfont"              # nerdfont | unicode | ascii
animations = true
list_density = "comfortable"    # comfortable | compact
statusline_separator = "arrow"  # arrow | round | slant | none
show_hints = true               # key hints next to the active field
date_format = "relative"        # relative | iso
mouse = true

[security]
clipboard_timeout = "30s"       # "never" keeps copied values
clear_clipboard_on_exit = true
auto_lock = "15m"               # "never" disables
reveal_timeout = "20s"
hide_on_focus_lost = true
reprompt_grace = "0s"           # how long a master password re-prompt stays valid
totp = true                     # show verification codes computed from authenticator keys

[vault]
sync_interval = "10m"           # "never" disables background sync
sort = "name"                   # name | modified | created | type
search_notes = false
show_empty_types = false
confirm_trash = false           # dd asks before moving to trash (it can always be undone with u)

[generator]
type = "password"               # password | passphrase
length = 24
uppercase = true
lowercase = true
numbers = true
special = true
min_numbers = 1
min_special = 1
avoid_ambiguous = false
words = 5
separator = "-"
capitalize = true
include_number = true

[health]
check_exposed = true            # Have I Been Pwned (k-anonymity) through bw report
old_password_age = "365d"

[editor]
command = ""                    # empty = $VISUAL, then $EDITOR, then vi

[keys]
leader = "<Space>"
timeoutlen = "1000ms"
which_key_delay = "300ms"
defaults = true
# Per-context overrides, e.g.:
# [keys.list]
# "<Del>" = "item.trash"
# "dd" = "noop"
```
