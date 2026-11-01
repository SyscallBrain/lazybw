//! The key dispatcher: pending sequences, counts, timeouts and which-key.
//!
//! It turns key presses into [`Action`] invocations using the keymap's trie.
//! Text-entry contexts get printable characters back so the caller can insert
//! them; every other unbound key is returned as `Unhandled` for the screen to
//! deal with (arrow keys in a picker, for example).

use crate::app::state::Action;
use crate::config::keymap::{ActionCall, Key, Keymap, Resolution, display_keys, hint};
use std::time::{Duration, Instant};

/// What the dispatcher decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// Run this action.
    Action(Action),
    /// Waiting for more keys.
    Pending,
    /// Insert this character (text-entry contexts).
    Text(char),
    /// A key with no binding; the screen may handle it.
    Unhandled(Key),
    /// Nothing to do.
    Discard,
}

/// The dispatcher state.
#[derive(Debug, Clone, Default)]
pub struct Dispatcher {
    /// Keys typed so far.
    pub pending: Vec<Key>,
    /// When the first pending key arrived.
    pub since: Option<Instant>,
    /// Count prefix.
    pub count: Option<u32>,
    /// Digits typed as part of the count.
    count_buffer: String,
}

impl Dispatcher {
    /// True when keys are pending.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Clears pending keys and the count.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.since = None;
        self.count = None;
        self.count_buffer.clear();
    }

    /// The pending sequence in display form.
    #[must_use]
    pub fn pending_display(&self, keymap: &Keymap) -> String {
        let mut out = String::new();
        if let Some(count) = &self.count {
            out.push_str(&count.to_string());
        }
        let pending = display_keys(&self.pending);
        if !pending.is_empty() {
            out.push_str(&pending);
        }
        let leader = display_keys(&keymap.leader);
        if out.starts_with(&leader) && !leader.is_empty() {
            out = out.replacen(&leader, "Space", 1);
        }
        out
    }

    /// Converts a resolved call to an action, applying the count.
    fn action(&self, call: ActionCall) -> Action {
        let count = if crate::app::actions::accepts_count(&call.id) { self.count } else { None };
        Action { id: call.id, args: call.args, count }
    }

    /// Feeds a key.
    pub fn feed(
        &mut self,
        key: Key,
        keymap: &Keymap,
        chain: &[String],
        text_context: Option<&str>,
        now: Instant,
    ) -> Dispatch {
        // Count prefix.
        if self.pending.is_empty()
            && let Key { code: crate::config::keymap::Code::Char(c), mods } = key
            && mods.is_empty()
            && c.is_ascii_digit()
        {
            let no_binding =
                text_context.is_none() && matches!(keymap.resolve(chain, &[key]), Resolution::None);
            if no_binding {
                if self.count_buffer.is_empty() && c == '0' {
                    // `0` alone is a motion, not a count.
                } else {
                    self.count_buffer.push(c);
                    self.count = self.count_buffer.parse::<u32>().ok();
                    return Dispatch::Pending;
                }
            }
        }

        // A printable key in a text-entry context is text unless the text
        // context itself binds it.
        if let Some(context) = text_context
            && key.is_printable()
        {
            let local = keymap.resolve(&[context.to_owned()], &[key]);
            if matches!(local, Resolution::None) {
                let ch = match key.code {
                    crate::config::keymap::Code::Char(c) => c,
                    _ => return Dispatch::Discard,
                };
                return Dispatch::Text(ch);
            }
        }

        let mut candidate = self.pending.clone();
        candidate.push(key);
        if self.since.is_none() {
            self.since = Some(now);
        }
        match keymap.resolve(chain, &candidate) {
            Resolution::Exact(call) => {
                let has_longer = keymap.continuations(chain, &candidate).iter().any(|_| true);
                if has_longer {
                    self.pending = candidate;
                    return Dispatch::Pending;
                }
                let action = self.action(call);
                self.reset();
                Dispatch::Action(action)
            }
            Resolution::Prefix => {
                self.pending = candidate;
                Dispatch::Pending
            }
            Resolution::None => {
                // A failed sequence is cancelled as a whole, like Vim: `za`
                // where only `zz` exists must not run `a` on its own. Only
                // when the pending keys were already a complete binding
                // (waiting for a longer one) is the key retried alone.
                let previous = std::mem::take(&mut self.pending);
                self.since = None;
                if !previous.is_empty() {
                    if matches!(keymap.resolve(chain, &previous), Resolution::Exact(_)) {
                        return self.feed(key, keymap, chain, text_context, now);
                    }
                    self.reset();
                    return Dispatch::Discard;
                }
                if key.is_printable()
                    && let Key { code: crate::config::keymap::Code::Char(c), .. } = key
                    && text_context.is_some()
                {
                    return Dispatch::Text(c);
                }
                Dispatch::Unhandled(key)
            }
        }
    }

    /// Resolves a complete-but-ambiguous pending sequence after `timeoutlen`.
    pub fn timeout(&mut self, keymap: &Keymap, chain: &[String]) -> Option<Action> {
        if self.pending.is_empty() {
            return None;
        }
        match keymap.resolve(chain, &self.pending) {
            Resolution::Exact(call) => {
                let action = self.action(call);
                self.reset();
                Some(action)
            }
            _ => {
                self.reset();
                None
            }
        }
    }

    /// True when the pending keys are themselves a complete binding, so
    /// `timeoutlen` should resolve them. A pure prefix (such as the leader)
    /// waits for the next key instead, keeping which-key on screen.
    #[must_use]
    pub fn timeout_applies(&self, keymap: &Keymap, chain: &[String]) -> bool {
        !self.pending.is_empty()
            && matches!(keymap.resolve(chain, &self.pending), Resolution::Exact(_))
    }

    /// True when the pending sequence should open which-key.
    #[must_use]
    pub fn which_key_due(&self, now: Instant, delay: Duration) -> bool {
        !self.pending.is_empty()
            && self.since.is_some_and(|since| now.duration_since(since) >= delay)
    }

    /// The continuations for which-key.
    #[must_use]
    pub fn continuations(&self, keymap: &Keymap, chain: &[String]) -> Vec<(String, bool, String)> {
        keymap
            .continuations(chain, &self.pending)
            .into_iter()
            .map(|(key, group, action)| {
                let mut prefix = self.pending.clone();
                prefix.push(key);
                let label_key: String = prefix.iter().map(|k| k.notation()).collect();
                let label = if group && action.is_empty() {
                    keymap.labels.get(&label_key).cloned().unwrap_or_else(|| "+more".to_owned())
                } else if action.is_empty() {
                    keymap.labels.get(&label_key).cloned().unwrap_or_default()
                } else {
                    let call = ActionCall::parse(&action, crate::config::keymap::Source::Default);
                    let description = crate::config::keymap::action_description(&call);
                    match (call.id.as_str(), call.args.first().map(String::as_str)) {
                        ("tab.goto", Some("1")) => "Vault tab".to_owned(),
                        ("tab.goto", Some("2")) => "Generator tab".to_owned(),
                        ("tab.goto", Some("3")) => "Health tab".to_owned(),
                        _ => description,
                    }
                };
                (hint(key), group && action.is_empty(), label)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeysConfig;
    use crate::config::keymap::{Keymap, chain, parse_keys};

    fn keymap() -> Keymap {
        Keymap::build(&KeysConfig::default()).unwrap().0
    }

    fn keys(s: &str) -> Vec<Key> {
        parse_keys(s).unwrap()
    }

    #[test]
    fn immediately_runs_unambiguous_bindings() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        assert_eq!(
            dispatcher.feed(Key::ch('a'), &km, &chain, None, Instant::now()),
            Dispatch::Action(Action::with("item.new", &[]))
        );
        assert!(!dispatcher.is_pending());
    }

    #[test]
    fn waits_for_ambiguous_sequences() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        assert_eq!(
            dispatcher.feed(Key::ch('d'), &km, &chain, None, Instant::now()),
            Dispatch::Pending
        );
        assert_eq!(dispatcher.pending_display(&km), "d");
        assert_eq!(
            dispatcher.feed(Key::ch('d'), &km, &chain, None, Instant::now()),
            Dispatch::Action(Action::new("item.trash"))
        );
    }

    #[test]
    fn counts_apply_to_actions_that_accept_them() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        assert_eq!(
            dispatcher.feed(Key::ch('3'), &km, &chain, None, Instant::now()),
            Dispatch::Pending
        );
        assert_eq!(
            dispatcher.feed(Key::ch('j'), &km, &chain, None, Instant::now()),
            Dispatch::Action(Action::new("nav.down").counted(Some(3)))
        );
    }

    #[test]
    fn zero_is_not_a_count_at_the_start() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["cmdline"]);
        assert_eq!(
            dispatcher.feed(Key::ch('0'), &km, &chain, Some("cmdline"), Instant::now()),
            Dispatch::Text('0')
        );
    }

    #[test]
    fn counts_are_ignored_by_other_actions() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        dispatcher.feed(Key::ch('2'), &km, &chain, None, Instant::now());
        let dispatch = dispatcher.feed(Key::ch('x'), &km, &chain, None, Instant::now());
        assert_eq!(dispatch, Dispatch::Action(Action::new("mark.toggle")));
    }

    #[test]
    fn timeout_resolves_complete_prefixes() {
        let mut dispatcher = Dispatcher::default();
        let mut km = keymap();
        let chain = chain(&["list"]);
        km.bind(
            "list",
            &keys("x"),
            crate::config::keymap::ActionCall::parse(
                "nav.down",
                crate::config::keymap::Source::User,
            ),
        );
        km.bind(
            "list",
            &keys("xx"),
            crate::config::keymap::ActionCall::parse("nav.up", crate::config::keymap::Source::User),
        );
        dispatcher.feed(Key::ch('x'), &km, &chain, None, Instant::now());
        assert!(dispatcher.is_pending());
        assert_eq!(dispatcher.timeout(&km, &chain), Some(Action::new("nav.down")));
        assert!(!dispatcher.is_pending());
    }

    #[test]
    fn which_key_is_due_after_the_delay() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        let start = Instant::now();
        dispatcher.feed(Key::ch('g'), &km, &chain, None, start);
        assert!(!dispatcher.which_key_due(start, Duration::from_millis(300)));
        assert!(
            dispatcher
                .which_key_due(start + Duration::from_millis(400), Duration::from_millis(300))
        );
        let continuations = dispatcher.continuations(&km, &chain);
        assert!(continuations.iter().any(|(key, _, _)| key == "g"));
    }

    #[test]
    fn failed_sequences_are_cancelled_whole() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        dispatcher.feed(Key::ch('g'), &km, &chain, None, Instant::now());
        // F10 completes nothing, so the whole `g F10` is cancelled.
        let key = Key::named(crate::config::keymap::Code::F(10));
        let dispatch = dispatcher.feed(key, &km, &chain, None, Instant::now());
        assert_eq!(dispatch, Dispatch::Discard);
        assert!(!dispatcher.is_pending());
        // `z` then `k` (no `zk` in the list): `k` must not move on its own.
        dispatcher.feed(Key::ch('z'), &km, &chain, None, Instant::now());
        let dispatch = dispatcher.feed(Key::ch('k'), &km, &chain, None, Instant::now());
        assert_eq!(dispatch, Dispatch::Discard);
        // The next key works normally again.
        assert!(matches!(
            dispatcher.feed(Key::ch('k'), &km, &chain, None, Instant::now()),
            Dispatch::Action(_)
        ));
    }

    #[test]
    fn text_contexts_insert_printable_keys() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["cmdline"]);
        assert_eq!(
            dispatcher.feed(Key::ch('j'), &km, &chain, Some("cmdline"), Instant::now()),
            Dispatch::Text('j')
        );
        assert_eq!(
            dispatcher.feed(
                Key::named(crate::config::keymap::Code::Enter),
                &km,
                &chain,
                Some("cmdline"),
                Instant::now()
            ),
            Dispatch::Action(Action::new("cmdline.execute"))
        );
    }

    #[test]
    fn unbound_named_keys_are_unhandled() {
        let mut dispatcher = Dispatcher::default();
        let km = keymap();
        let chain = chain(&["list"]);
        let key = Key::named(crate::config::keymap::Code::F(10));
        assert_eq!(
            dispatcher.feed(key, &km, &chain, None, Instant::now()),
            Dispatch::Unhandled(key)
        );
    }
}
