//! Icons: Nerd Font glyphs, Unicode symbols and an ASCII fallback.
//!
//! Every role has one glyph per mode; widgets ask for a role, never a
//! codepoint. The Nerd Font table is in the Private Use Area and each role is
//! distinct.

/// A visual role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Login item.
    Login,
    /// Card item.
    Card,
    /// Identity item.
    Identity,
    /// Secure note.
    Note,
    /// SSH key.
    Ssh,
    /// Unknown item type.
    Other,
    /// All items.
    All,
    /// Favourite.
    Favorite,
    /// TOTP.
    Totp,
    /// Passkey.
    Passkey,
    /// Attachment.
    Attachment,
    /// Warning.
    Warning,
    /// Master password re-prompt.
    Reprompt,
    /// Closed folder.
    FolderClosed,
    /// Open folder.
    FolderOpen,
    /// Trash.
    Trash,
    /// Archive.
    Archive,
    /// Sync.
    Sync,
    /// Clipboard.
    Clipboard,
    /// Lock.
    Lock,
    /// Unlock.
    Unlock,
    /// Website.
    Website,
    /// Check mark.
    Check,
    /// Cross.
    Cross,
    /// Generator tab.
    Generator,
    /// Health tab.
    Health,
    /// Settings.
    Settings,
    /// Keybindings.
    Keyboard,
    /// The app logo in the topbar (a padlock, like the project logo).
    Logo,
}

impl Role {
    /// The Nerd Font codepoint.
    #[must_use]
    pub fn nerdfont(self) -> char {
        match self {
            Self::Login => '\u{f084}',
            Self::Card => '\u{f09d}',
            Self::Identity => '\u{f2c2}',
            Self::Note => '\u{f249}',
            Self::Ssh => '\u{f120}',
            Self::Other => '\u{f059}',
            Self::All => '\u{f0c9}',
            Self::Favorite => '\u{f005}',
            Self::Totp => '\u{f017}',
            Self::Passkey => '\u{f132}',
            Self::Attachment => '\u{f0c6}',
            Self::Warning => '\u{f071}',
            Self::Reprompt => '\u{f023}',
            Self::FolderClosed => '\u{f07b}',
            Self::FolderOpen => '\u{f07c}',
            Self::Trash => '\u{f1f8}',
            Self::Archive => '\u{f187}',
            Self::Sync => '\u{f021}',
            Self::Clipboard => '\u{f0ea}',
            Self::Lock => '\u{f023}',
            Self::Unlock => '\u{f09c}',
            Self::Website => '\u{f0ac}',
            Self::Check => '\u{f00c}',
            Self::Cross => '\u{f00d}',
            Self::Generator => '\u{f0d0}',
            Self::Health => '\u{f21e}',
            Self::Settings => '\u{f013}',
            Self::Keyboard => '\u{f11c}',
            Self::Logo => '\u{f0341}',
        }
    }

    /// The Unicode symbol.
    #[must_use]
    pub fn unicode(self) -> char {
        match self {
            Self::Login => '●',
            Self::Card => '■',
            Self::Identity => '▲',
            Self::Note => '◆',
            Self::Ssh => '◇',
            Self::Other => '?',
            Self::All => '≡',
            Self::Favorite => '★',
            Self::Totp => '◷',
            Self::Passkey => '◈',
            Self::Attachment => '⊞',
            Self::Warning => '⚠',
            Self::Reprompt => '⚿',
            Self::FolderClosed => '▸',
            Self::FolderOpen => '▾',
            Self::Trash => '✕',
            Self::Archive => '⊟',
            Self::Sync => '⟳',
            Self::Clipboard => '⧗',
            Self::Lock => '⚿',
            Self::Unlock => '●',
            Self::Website => '↗',
            Self::Check => '✓',
            Self::Cross => '✗',
            Self::Generator => '✦',
            Self::Health => '♥',
            Self::Settings => '⚙',
            Self::Keyboard => '⌨',
            Self::Logo => '◆',
        }
    }

    /// The ASCII fallback.
    #[must_use]
    pub fn ascii(self) -> char {
        match self {
            Self::Login => 'L',
            Self::Card => 'C',
            Self::Identity => 'I',
            Self::Note => 'N',
            Self::Ssh => 'K',
            Self::Other => '?',
            Self::All => '=',
            Self::Favorite => '*',
            Self::Totp => 'T',
            Self::Passkey => 'P',
            Self::Attachment => '+',
            Self::Warning => '!',
            Self::Reprompt => 'R',
            Self::FolderClosed => '>',
            Self::FolderOpen => 'v',
            Self::Trash => 'x',
            Self::Archive => 'a',
            Self::Sync => '~',
            Self::Clipboard => 'c',
            Self::Lock => 'L',
            Self::Unlock => 'U',
            Self::Website => '>',
            Self::Check => 'v',
            Self::Cross => 'x',
            Self::Generator => 'G',
            Self::Health => 'H',
            Self::Settings => 'S',
            Self::Keyboard => 'K',
            Self::Logo => '*',
        }
    }
}

/// The icon set resolved from configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    /// The mode.
    pub mode: crate::config::IconMode,
}

impl Icons {
    /// Builds from the config.
    #[must_use]
    pub fn new(mode: crate::config::IconMode) -> Self {
        Self { mode }
    }

    /// The glyph for a role.
    #[must_use]
    pub fn get(self, role: Role) -> char {
        match self.mode {
            crate::config::IconMode::Nerdfont => role.nerdfont(),
            crate::config::IconMode::Unicode => role.unicode(),
            crate::config::IconMode::Ascii => role.ascii(),
        }
    }

    /// The role for an item type.
    #[must_use]
    pub fn item(kind: crate::bw::models::ItemType) -> Role {
        use crate::bw::models::ItemType;
        match kind {
            ItemType::Login => Role::Login,
            ItemType::Card => Role::Card,
            ItemType::Identity => Role::Identity,
            ItemType::SecureNote => Role::Note,
            ItemType::SshKey => Role::Ssh,
            ItemType::Other(_) => Role::Other,
        }
    }

    /// A brand glyph for a registrable domain, in Nerd Font mode only.
    #[must_use]
    pub fn brand(self, domain: &str) -> Option<char> {
        if self.mode != crate::config::IconMode::Nerdfont {
            return None;
        }
        brand_glyph(domain)
    }

    /// The powerline separator pair for a style.
    #[must_use]
    pub fn separator(self, style: crate::config::SeparatorStyle) -> (char, char) {
        if self.mode != crate::config::IconMode::Nerdfont {
            return (' ', ' ');
        }
        match style {
            crate::config::SeparatorStyle::Arrow => ('\u{e0b0}', '\u{e0b2}'),
            crate::config::SeparatorStyle::Round => ('\u{e0b4}', '\u{e0b6}'),
            crate::config::SeparatorStyle::Slant => ('\u{e0bc}', '\u{e0ba}'),
            crate::config::SeparatorStyle::None => (' ', ' '),
        }
    }
}

fn brand_glyph(domain: &str) -> Option<char> {
    let glyph = match domain {
        "github.com" => '\u{f09b}',
        "gitlab.com" => '\u{f296}',
        "google.com" | "gmail.com" => '\u{f1a0}',
        "apple.com" | "icloud.com" => '\u{f179}',
        "microsoft.com" | "live.com" | "outlook.com" => '\u{f17a}',
        "reddit.com" => '\u{f1a1}',
        "twitter.com" | "x.com" => '\u{f099}',
        "facebook.com" => '\u{f09a}',
        "linkedin.com" => '\u{f0e1}',
        "steampowered.com" => '\u{f1b6}',
        "spotify.com" => '\u{f1bc}',
        "slack.com" => '\u{f198}',
        "dropbox.com" => '\u{f16b}',
        "paypal.com" => '\u{f1ed}',
        "stackoverflow.com" => '\u{f16c}',
        "youtube.com" => '\u{f167}',
        "twitch.tv" => '\u{f1e8}',
        "wikipedia.org" => '\u{f266}',
        "bitbucket.org" => '\u{f171}',
        "trello.com" => '\u{f181}',
        "telegram.org" => '\u{f2c6}',
        "discord.com" => '\u{f1ff}',
        "docker.com" => '\u{f308}',
        "cloudflare.com" => '\u{f0c2}',
        "netflix.com" => '\u{f1c9}',
        _ if domain.starts_with("amazon.") => '\u{f270}',
        _ => return None,
    };
    Some(glyph)
}

/// The spinner frames.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The frame for a given tick.
#[must_use]
pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER[tick % SPINNER.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nerd_font_glyphs_are_private_use_and_distinct() {
        let roles = [
            Role::Login,
            Role::Card,
            Role::Identity,
            Role::Note,
            Role::Ssh,
            Role::Other,
            Role::All,
            Role::Favorite,
            Role::Totp,
            Role::Passkey,
            Role::Attachment,
            Role::Warning,
            Role::Reprompt,
            Role::FolderClosed,
            Role::FolderOpen,
            Role::Trash,
            Role::Archive,
            Role::Sync,
            Role::Clipboard,
            Role::Website,
            Role::Check,
            Role::Cross,
            Role::Generator,
            Role::Health,
            Role::Settings,
            Role::Keyboard,
        ];
        let mut seen = HashSet::new();
        for role in roles {
            let glyph = role.nerdfont();
            let code = glyph as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&code)
                    || (0xF0000..=0xFFFFD).contains(&code)
                    || (0xE0000..=0xE007F).contains(&code),
                "{role:?} {code:x} is not in a private use area"
            );
            assert!(seen.insert(glyph), "duplicate glyph for {role:?}");
        }
    }

    #[test]
    fn every_role_has_all_three_modes() {
        for role in [Role::Login, Role::Favorite, Role::Trash, Role::Logo] {
            let _ = role.nerdfont();
            let _ = role.unicode();
            let _ = role.ascii();
        }
    }

    #[test]
    fn brand_only_in_nerdfont_mode() {
        let nerd = Icons::new(crate::config::IconMode::Nerdfont);
        let ascii = Icons::new(crate::config::IconMode::Ascii);
        assert_eq!(nerd.brand("github.com"), Some('\u{f09b}'));
        assert_eq!(ascii.brand("github.com"), None);
        assert_eq!(nerd.brand("example.com"), None);
    }

    #[test]
    fn separators_fall_back_for_unicode() {
        let unicode = Icons::new(crate::config::IconMode::Unicode);
        assert_eq!(unicode.separator(crate::config::SeparatorStyle::Arrow), (' ', ' '));
    }
}
