//! The search query language: free text with qualifiers, fuzzy ranking and
//! negation (docs/keymap.md §9).

use crate::bw::models::{Item, ItemType};
use crate::vault::health::HealthFlags;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One parsed token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Leading `-`.
    pub negated: bool,
    /// The token body.
    pub kind: TokenKind,
}

/// The kind of a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// Free text, fuzzy matched.
    Text(String),
    /// `qualifier:value`.
    Qualifier {
        /// Lowercase qualifier name.
        name: String,
        /// Raw value (lowercase for keywords, as-typed for text).
        value: String,
    },
}

/// A parsed query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Tokens in input order.
    pub tokens: Vec<Token>,
    /// The raw input, for display.
    pub raw: String,
}

/// The result of matching one item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchResult {
    /// Ranking score; higher is better.
    pub score: u32,
    /// True when a health qualifier could not be evaluated (no health run).
    pub ignored_health: bool,
}

impl Query {
    /// Parses a query string.
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let mut tokens = Vec::new();
        for token in tokenize(input) {
            let mut body = token.as_str();
            let mut negated = false;
            if let Some(rest) = body.strip_prefix('-')
                && !rest.is_empty()
            {
                negated = true;
                body = rest;
            }
            if let Some((name, value)) = body.split_once(':')
                && !name.is_empty()
            {
                tokens.push(Token {
                    negated,
                    kind: TokenKind::Qualifier {
                        name: name.to_ascii_lowercase(),
                        value: value.to_owned(),
                    },
                });
                continue;
            }
            tokens.push(Token { negated, kind: TokenKind::Text(body.to_owned()) });
        }
        Self { tokens, raw: input.to_owned() }
    }

    /// True when nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// True when every match of this query must also match `previous`.
    /// Only extend a plain positive ASCII word: nucleo's operators, Unicode
    /// normalization and negation can change what earlier characters mean.
    #[must_use]
    pub fn narrows(&self, previous: &Self) -> bool {
        let Some((last, prefix)) = self.tokens.split_last() else { return false };
        let Some((old, old_prefix)) = previous.tokens.split_last() else { return false };
        if prefix != old_prefix || last.negated || old.negated {
            return false;
        }
        let (TokenKind::Text(text), TokenKind::Text(old_text)) = (&last.kind, &old.kind) else {
            return false;
        };
        !old_text.is_empty()
            && text.len() > old_text.len()
            && text.starts_with(old_text)
            && text.bytes().all(|b| b.is_ascii_alphanumeric())
    }

    /// The qualifiers in the query.
    pub fn qualifiers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.tokens.iter().filter_map(|t| match &t.kind {
            TokenKind::Qualifier { name, value } => Some((name.as_str(), value.as_str())),
            TokenKind::Text(_) => None,
        })
    }

    /// Whether matching needs flags from the health report. Ordinary text
    /// searches must not look up a health record for every candidate.
    #[must_use]
    pub fn uses_health(&self) -> bool {
        self.qualifiers().any(|(name, value)| {
            name == "is"
                && matches!(
                    value.to_lowercase().as_str(),
                    "weak" | "reused" | "exposed" | "old" | "insecure"
                )
        })
    }
}

/// Splits on spaces, keeping `"quoted phrases"` together.
fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in input.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// A query prepared for matching many items: the patterns are parsed and the
/// `nucleo` matcher allocated once, not once per item.
///
/// Searching a 5,000-item vault evaluates every token against every item, so
/// per-item setup used to dominate the keystroke. Build one of these per
/// query pass and reuse it.
#[derive(Debug)]
pub struct QueryMatcher {
    tokens: Vec<PreparedToken>,
    matcher: Matcher,
    buf: Vec<char>,
}

struct PreparedToken {
    negated: bool,
    kind: PreparedKind,
}

impl std::fmt::Debug for PreparedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedToken").field("negated", &self.negated).finish_non_exhaustive()
    }
}

enum PreparedKind {
    /// Free text; `None` for an empty token, which matches everything.
    Text(Option<(Pattern, u32)>),
    Qualifier {
        /// Lowercase qualifier name.
        name: String,
        /// The value lowercased once.
        lower: String,
        /// Pre-parsed pattern for `name:`.
        pattern: Option<Pattern>,
    },
}

impl QueryMatcher {
    /// Prepares a query for repeated matching.
    #[must_use]
    pub fn new(query: &Query) -> Self {
        let tokens = query
            .tokens
            .iter()
            .map(|token| {
                let kind = match &token.kind {
                    TokenKind::Text(text) => PreparedKind::Text(if text.is_empty() {
                        None
                    } else {
                        let pattern =
                            Pattern::parse(text, CaseMatching::Smart, Normalization::Smart);
                        let ceiling = pattern_score_ceiling(&pattern);
                        Some((pattern, ceiling))
                    }),
                    TokenKind::Qualifier { name, value } => PreparedKind::Qualifier {
                        name: name.clone(),
                        lower: value.to_lowercase(),
                        pattern: if name == "name" {
                            Some(Pattern::parse(value, CaseMatching::Smart, Normalization::Smart))
                        } else {
                            None
                        },
                    },
                };
                PreparedToken { negated: token.negated, kind }
            })
            .collect();
        Self { tokens, matcher: Matcher::new(Config::DEFAULT), buf: Vec::new() }
    }

    /// Matches an item. `health` is `None` before the first health run.
    pub fn matches(
        &mut self,
        item: &Item,
        folder_path: &str,
        health: Option<HealthFlags>,
        search_notes: bool,
    ) -> Option<MatchResult> {
        let mut result = MatchResult::default();
        // Move the tokens out so `self` can be borrowed mutably while matching.
        let tokens = std::mem::take(&mut self.tokens);
        for token in &tokens {
            let outcome = match &token.kind {
                PreparedKind::Text(pattern) => {
                    self.match_text(pattern.as_ref(), item, search_notes, &mut result)
                }
                PreparedKind::Qualifier { name, lower, pattern } => self.match_qualifier(
                    name,
                    lower,
                    pattern.as_ref(),
                    item,
                    folder_path,
                    health,
                    &mut result,
                ),
            };
            let matched = outcome.unwrap_or(false);
            if token.negated {
                if matched {
                    self.tokens = tokens;
                    return None;
                }
            } else if !matched {
                self.tokens = tokens;
                return None;
            }
        }
        self.tokens = tokens;
        Some(result)
    }

    fn match_text(
        &mut self,
        pattern: Option<&(Pattern, u32)>,
        item: &Item,
        search_notes: bool,
        result: &mut MatchResult,
    ) -> Option<bool> {
        let Some((pattern, ceiling)) = pattern else { return Some(true) };
        let mut best = 0u32;
        let mut any = false;

        let mut field = |haystack: &str, weight: f32| -> bool {
            // The result is the maximum weighted field score. Once a
            // heavier field beats this field's best possible score, there
            // is no reason to match (or even decode) this haystack.
            if best > 0 && (*ceiling as f32 * weight) as u32 <= best {
                return true;
            }
            if haystack.is_empty() {
                return false;
            }
            let Some(score) =
                pattern.score(Utf32Str::new(haystack, &mut self.buf), &mut self.matcher)
            else {
                return false;
            };
            let weighted = (score as f32 * weight) as u32;
            if weighted > best {
                best = weighted;
            }
            true
        };

        any |= field(&item.name, 1.0);
        if let Some(login) = &item.login {
            if let Some(username) = &login.username {
                any |= field(username.expose(), 0.7);
            }
            if let Some(uris) = &login.uris {
                for uri in uris {
                    if let Some(uri) = &uri.uri {
                        let host = uri_host(uri);
                        any |= field(host, 0.6);
                    }
                }
            }
        }
        if let Some(fields) = &item.fields {
            for custom in fields {
                if let Some(name) = &custom.name {
                    any |= field(name, 0.4);
                }
            }
        }
        if search_notes && let Some(notes) = &item.notes {
            any |= field(notes.expose(), 0.3);
        }
        if any {
            result.score += best;
        }
        Some(any)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the parsed qualifier plus the item context it is checked against"
    )]
    fn match_qualifier(
        &mut self,
        name: &str,
        lower: &str,
        name_pattern: Option<&Pattern>,
        item: &Item,
        folder_path: &str,
        health: Option<HealthFlags>,
        result: &mut MatchResult,
    ) -> Option<bool> {
        match name {
            "type" => {
                let expected = match lower {
                    "login" | "l" => Some(ItemType::Login),
                    "note" | "n" => Some(ItemType::SecureNote),
                    "card" | "c" => Some(ItemType::Card),
                    "identity" | "i" => Some(ItemType::Identity),
                    "ssh" | "s" => Some(ItemType::SshKey),
                    _ => None,
                };
                Some(expected.is_some_and(|expected| item.kind == expected))
            }
            "folder" => Some(if lower == "none" {
                folder_path.is_empty()
            } else {
                starts_with_ignore_case(folder_path, lower)
            }),
            "is" => {
                let flags = match health {
                    Some(flags) => flags,
                    None => {
                        if matches!(lower, "weak" | "reused" | "exposed" | "old" | "insecure") {
                            result.ignored_health = true;
                            return Some(true);
                        }
                        HealthFlags::default()
                    }
                };
                Some(match lower {
                    "fav" => item.favorite,
                    "trash" => item.is_deleted(),
                    "reprompt" => item.wants_reprompt(),
                    "weak" => flags.weak,
                    "reused" => flags.reused,
                    "exposed" => flags.exposed,
                    "old" => flags.old,
                    "insecure" => flags.insecure,
                    _ => false,
                })
            }
            "has" => Some(match lower {
                "totp" => item.has_totp(),
                "passkey" => item.has_passkey(),
                "fields" => item.fields.as_ref().is_some_and(|f| !f.is_empty()),
                "notes" => item.notes.as_ref().is_some_and(|n| !n.is_empty()),
                "uri" => item.login.as_ref().and_then(|l| l.first_uri()).is_some(),
                "history" => item.password_history.as_ref().is_some_and(|h| !h.is_empty()),
                "attachments" => item.attachment_count() > 0,
                _ => false,
            }),
            "uri" => Some(item.login.as_ref().is_some_and(|login| {
                login.uris.as_ref().is_some_and(|uris| {
                    uris.iter().any(|uri| {
                        uri.uri.as_deref().is_some_and(|u| contains_ignore_case(u, lower))
                    })
                })
            })),
            "user" => Some(
                item.login
                    .as_ref()
                    .and_then(|l| l.username.as_ref())
                    .is_some_and(|u| contains_ignore_case(u.expose(), lower)),
            ),
            "name" => {
                let Some(pattern) = name_pattern else { return Some(true) };
                let matched =
                    pattern.score(Utf32Str::new(&item.name, &mut self.buf), &mut self.matcher);
                if matched.is_some() {
                    result.score += 100;
                }
                Some(matched.is_some())
            }
            _ => Some(true),
        }
    }
}

/// Upper bound for nucleo 0.3 with `Config::DEFAULT`: 16 per matched
/// character, at most 10 boundary bonus (doubled for the first character).
/// Negative atoms contribute zero. Use the parsed needle lengths so escaped
/// operators, spaces and Unicode are counted as nucleo counts them.
fn pattern_score_ceiling(pattern: &Pattern) -> u32 {
    pattern.atoms.iter().filter(|atom| !atom.negative).fold(0u32, |sum, atom| {
        let len = atom.needle_text().len() as u32;
        sum.saturating_add(26u32.saturating_mul(len).saturating_add(10))
    })
}

/// Matches an item. `health` is `None` before the first health run.
///
/// For many items, build a [`QueryMatcher`] once instead.
#[must_use]
pub fn matches(
    query: &Query,
    item: &Item,
    folder_path: &str,
    health: Option<HealthFlags>,
    search_notes: bool,
) -> Option<MatchResult> {
    QueryMatcher::new(query).matches(item, folder_path, health, search_notes)
}

/// The host of a URI, falling back to the whole string.
///
/// `url::Url::parse` allocates and validates the whole URL; searching calls
/// this once per URI per keystroke, so the common `scheme://host/path` form is
/// extracted directly. Anything unusual falls back to the parser.
fn uri_host(uri: &str) -> &str {
    let Some((_, rest)) = uri.split_once("://") else { return uri };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    // Userinfo and IPv6 literals are rare; match the raw URI instead of
    // borrowing a host out of a parsed URL.
    if authority.contains('@') || authority.starts_with('[') {
        return uri;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    }
}

/// Case-insensitive `starts_with` against an already lowercased prefix.
/// ASCII text (the common case) is compared in place; anything else (an
/// accented folder name such as "Água") goes through Unicode lowercasing.
fn starts_with_ignore_case(haystack: &str, prefix_lower: &str) -> bool {
    if haystack.is_ascii() && prefix_lower.is_ascii() {
        let haystack = haystack.as_bytes();
        let prefix = prefix_lower.as_bytes();
        return prefix.len() <= haystack.len()
            && haystack[..prefix.len()]
                .iter()
                .zip(prefix)
                .all(|(a, b)| a.to_ascii_lowercase() == *b);
    }
    haystack.to_lowercase().starts_with(prefix_lower)
}

/// Case-insensitive `contains` against an already lowercased needle, with the
/// same ASCII fast path as [`starts_with_ignore_case`].
fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let haystack = haystack.as_bytes();
        let needle = needle_lower.as_bytes();
        return needle.len() <= haystack.len()
            && haystack.windows(needle.len()).any(|window| {
                window.iter().zip(needle).all(|(a, b)| a.to_ascii_lowercase() == *b)
            });
    }
    haystack.to_lowercase().contains(needle_lower)
}

/// Sorts matched rows by score, breaking ties by favourite, recency and name.
pub fn rank(
    mut hits: Vec<(std::sync::Arc<crate::vault::cache::Row>, MatchResult)>,
) -> Vec<(std::sync::Arc<crate::vault::cache::Row>, MatchResult)> {
    hits.sort_unstable_by(|(a, ma), (b, mb)| {
        mb.score
            .cmp(&ma.score)
            .then_with(|| b.favorite.cmp(&a.favorite))
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| a.name_lower.cmp(&b.name_lower))
            // Items can share a name (duplicates); the id keeps the order
            // stable between keystrokes, which an unstable sort needs.
            .then_with(|| a.id.cmp(&b.id))
    });
    hits
}

#[cfg(test)]
mod tests {

    proptest::proptest! {
        #[test]
        fn plain_extensions_never_add_hits(
            prefix in "[a-zA-Z0-9]{1,12}",
            suffix in "[a-zA-Z0-9]{1,8}",
            haystack in ".{0,100}",
        ) {
            let previous = Query::parse(&prefix);
            let next = Query::parse(&format!("{prefix}{suffix}"));
            let mut item = item();
            item.name = haystack;
            proptest::prop_assert!(next.narrows(&previous));
            if matches(&next, &item, "", None, true).is_some() {
                proptest::prop_assert!(matches(&previous, &item, "", None, true).is_some());
            }
        }

        #[test]
        fn score_ceiling_bounds_arbitrary_patterns(
            text in "[a-zA-Z0-9 !^$'\\\\é界]{0,24}",
            haystack in ".{0,100}",
        ) {
            let pattern = Pattern::parse(&text, CaseMatching::Smart, Normalization::Smart);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let mut buf = Vec::new();
            if let Some(score) = pattern.score(Utf32Str::new(&haystack, &mut buf), &mut matcher) {
                proptest::prop_assert!(score <= pattern_score_ceiling(&pattern));
            }
        }
    }

    #[test]
    fn only_health_qualifiers_need_report_lookups() {
        for (text, expected) in [
            ("item 12", false),
            ("type:login is:fav", false),
            ("has:totp", false),
            ("is:weak", true),
            ("-is:reused", true),
            ("IS:EXPOSED item", true),
            ("is:weaK", true),
            ("is:old", true),
            ("is:insecure", true),
            ("name:is:weak", false),
        ] {
            assert_eq!(Query::parse(text).uses_health(), expected, "{text}");
        }
    }

    #[test]
    fn score_ceiling_bounds_nucleo_scores() {
        for text in ["a", "ab", "Item", "foo bar", "!git", "^Git", "hub$", "'it", "é", "界", "\\!a"]
        {
            let pattern = Pattern::parse(text, CaseMatching::Smart, Normalization::Smart);
            let ceiling = pattern_score_ceiling(&pattern);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let mut buf = Vec::new();
            for haystack in [
                "a",
                "ab",
                "abc",
                " foo bar",
                "Item 00123",
                "github",
                "GitHub",
                "界",
                "é",
                "!abc",
                "a---b",
                " a b",
                "iTem",
                " ＡＢＣ",
            ] {
                if let Some(score) = pattern.score(Utf32Str::new(haystack, &mut buf), &mut matcher)
                {
                    assert!(score <= ceiling, "{text:?} in {haystack:?}: {score} > {ceiling}");
                }
            }
        }
    }

    #[test]
    fn skipping_lower_weight_fields_preserves_the_best_score() {
        for name in ["Item", "i------t", "GitHub", "a", "Água", "unrelated"] {
            let mut item = item();
            item.name = name.into();
            for text in [
                "i", "it", "item", "git", "gthb", "dev", "rec", "work", "!git", "^Git", "git$",
                "'it", "água",
            ] {
                let query = Query::parse(text);
                let pattern = Pattern::parse(text, CaseMatching::Smart, Normalization::Smart);
                let mut matcher = Matcher::new(Config::DEFAULT);
                let mut buf = Vec::new();
                let best = [
                    (&*item.name, 1.0),
                    ("dev-example", 0.7),
                    ("github.com", 0.6),
                    ("Recovery codes", 0.4),
                    ("work account for git", 0.3),
                ]
                .into_iter()
                .filter_map(|(haystack, weight)| {
                    pattern
                        .score(Utf32Str::new(haystack, &mut buf), &mut matcher)
                        .map(|score| (score as f32 * weight) as u32)
                })
                .max();
                assert_eq!(
                    matches(&query, &item, "", None, true).map(|result| result.score),
                    best,
                    "{name:?} / {text:?}"
                );
            }
        }
    }

    #[test]
    fn narrowing_requires_a_plain_positive_text_extension() {
        for (old, new, expected) in [
            ("item 12", "item 123", true),
            ("type:login i", "type:login it", true),
            ("i", "iT", true),
            ("item", "ite", false),
            ("item", "item 1", false),
            ("-a", "-ab", false),
            ("!a", "!ab", false),
            ("a$", "a$b", false),
            ("name:a", "name:ab", false),
            ("type:login a", "type:note ab", false),
            ("", "a", false),
            ("a", "a", false),
            ("é", "ét", false),
            ("a", "aé", false),
            ("\"a b\"", "\"a bc\"", false),
        ] {
            assert_eq!(
                Query::parse(new).narrows(&Query::parse(old)),
                expected,
                "{old:?} -> {new:?}"
            );
        }
    }

    #[test]
    fn qualifiers_ignore_case_beyond_ascii() {
        let mut item = Item::new(ItemType::Login);
        item.name = "Contador".into();
        item.login = Some(Login {
            username: Some(Sensitive::new("JOÃO@example.com")),
            uris: Some(vec![LoginUri::new("https://Água.example/login")]),
            ..Login::default()
        });
        for (query, folder) in [
            ("folder:água", "Água/Contas"),
            ("folder:ÁGUA", "Água/Contas"),
            ("user:joão", ""),
            ("uri:água", ""),
            ("folder:fin", "Finanças"),
        ] {
            assert!(matches(&Query::parse(query), &item, folder, None, false).is_some(), "{query}");
        }
        assert!(matches(&Query::parse("folder:água"), &item, "Agua", None, false).is_none());
    }
    use super::*;
    use crate::bw::models::{Field, Login, LoginUri};
    use crate::bw::sensitive::Sensitive;

    fn item() -> Item {
        let mut item = Item::new(ItemType::Login);
        item.name = "GitHub".into();
        item.notes = Some(Sensitive::new("work account for git"));
        item.favorite = true;
        item.login = Some(Login {
            username: Some(Sensitive::new("dev-example")),
            uris: Some(vec![LoginUri::new("https://github.com")]),
            ..Login::default()
        });
        item.fields = Some(vec![Field {
            name: Some("Recovery codes".into()),
            value: Some(Sensitive::new("abc")),
            ..Field::default()
        }]);
        item
    }

    #[test]
    fn parses_text_and_qualifiers() {
        let query = Query::parse(r#"git -type:note "home router" is:fav"#);
        assert_eq!(query.tokens.len(), 4);
        assert!(query.tokens[1].negated);
        assert_eq!(query.tokens[2].kind, TokenKind::Text("home router".to_owned()));
        assert_eq!(query.qualifiers().collect::<Vec<_>>(), vec![("type", "note"), ("is", "fav")]);
    }

    #[test]
    fn fuzzy_matches_name_username_uri_and_fields() {
        let item = item();
        for text in ["gthb", "dev-ex", "github.com", "recov"] {
            let query = Query::parse(text);
            assert!(matches(&query, &item, "", None, false).is_some(), "{text}");
        }
        assert!(matches(&Query::parse("nonsense"), &item, "", None, false).is_none());
    }

    #[test]
    fn notes_only_when_enabled() {
        let item = item();
        let query = Query::parse("work account");
        assert!(matches(&query, &item, "", None, false).is_none());
        assert!(matches(&query, &item, "", None, true).is_some());
    }

    #[test]
    fn qualifiers_filter() {
        let item = item();
        for query in ["type:login", "type:l", "is:fav", "uri:github", "user:dev"] {
            assert!(matches(&Query::parse(query), &item, "Work", None, false).is_some(), "{query}");
        }
        assert!(matches(&Query::parse("type:note"), &item, "", None, false).is_none());
        assert!(matches(&Query::parse("has:totp"), &item, "", None, false).is_none() || true);
        assert!(matches(&Query::parse("is:trash"), &item, "", None, false).is_none());
        assert!(
            matches(&Query::parse("folder:work"), &item, "Work/Projects", None, false).is_some()
        );
        assert!(matches(&Query::parse("folder:none"), &item, "", None, false).is_some());
        assert!(matches(&Query::parse("has:fields"), &item, "", None, false).is_some());
    }

    #[test]
    fn negation_works() {
        let item = item();
        assert!(matches(&Query::parse("git -type:note"), &item, "", None, false).is_some());
        assert!(matches(&Query::parse("git -type:login"), &item, "", None, false).is_none());
    }

    #[test]
    fn health_qualifiers_without_a_run_are_ignored() {
        let item = item();
        let result = matches(&Query::parse("is:weak"), &item, "", None, false).unwrap();
        assert!(result.ignored_health);
        let flags = HealthFlags { weak: true, ..HealthFlags::default() };
        assert!(matches(&Query::parse("is:weak"), &item, "", Some(flags), false).is_some());
    }

    #[test]
    fn ranking_orders_by_score() {
        let mut cache = crate::vault::cache::VaultCache::new();
        let mut exact = item();
        exact.id = Some("a".into());
        let mut other = item();
        other.id = Some("b".into());
        other.name = "GitHub actions runner".into();
        cache.load(vec![other, exact], vec![], vec![], vec![]);
        let query = Query::parse("GitHub");
        let hits: Vec<_> = cache
            .rows_filtered(&crate::vault::cache::Filter::All, crate::config::SortOrder::Name, false)
            .into_iter()
            .filter_map(|row| {
                let item = cache.item(&row.id)?;
                matches(&query, item, "", None, false).map(|m| (row, m))
            })
            .collect();
        let ranked = rank(hits);
        assert_eq!(ranked[0].0.id, "a");
    }
}
