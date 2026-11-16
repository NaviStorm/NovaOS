use crate::{Error, Result};
use alloc::{string::ToString, vec::Vec};
use regex_automata::util::primitives::StateID;
use regex_automata::{
    Anchored, MatchKind,
    dfa::{Automaton, StartKind, dense},
    util::{start, syntax},
};

/// An anchored ERE can report every acceptable end at a given start. Retain
/// the last end before the DFA dies (POSIX longest), including at actual EOF.
pub(crate) struct Ere(dense::DFA<Vec<u32>>);
impl Ere {
    pub fn new(pattern: &str, raw: bool, insensitive: bool) -> Result<Self> {
        let mut builder = dense::Builder::new();
        builder.configure(
            dense::Config::new()
                .match_kind(MatchKind::All)
                .start_kind(StartKind::Anchored)
                .dfa_size_limit(Some(2 << 20))
                .determinize_size_limit(Some(4 << 20)),
        );
        builder.syntax(
            syntax::Config::new()
                .case_insensitive(insensitive)
                .dot_matches_new_line(true)
                .unicode(!raw)
                .utf8(!raw),
        );
        // In byte mode the parser stores octets 128..255 as Latin-1 scalars.
        let mut encoded = alloc::string::String::new();
        if raw {
            for c in pattern.chars() {
                if c.is_ascii() {
                    encoded.push(c);
                } else {
                    let byte =
                        u8::try_from(c as u32).map_err(|_| Error::runtime("invalid byte in RS"))?;
                    use core::fmt::Write;
                    write!(encoded, "\\x{byte:02x}").unwrap();
                }
            }
        }
        builder
            .build(if raw { &encoded } else { pattern })
            .map(Self)
            .map_err(|e| Error::runtime(e.to_string()))
    }
    pub fn start(&self, before: Option<u8>) -> Result<StateID> {
        self.0
            .start_state(
                &start::Config::new()
                    .anchored(Anchored::Yes)
                    .look_behind(before),
            )
            .map_err(|e| Error::runtime(e.to_string()))
    }
    pub fn step(&self, state: StateID, next: Option<u8>) -> StateID {
        match next {
            Some(byte) => self.0.next_state(state, byte),
            None => self.0.next_eoi_state(state),
        }
    }
    pub fn matched(&self, s: StateID) -> bool {
        self.0.is_match_state(s)
    }
    pub fn dead(&self, s: StateID) -> bool {
        self.0.is_dead_state(s)
    }
    pub fn end(&self, text: &[u8], begin: usize) -> Result<usize> {
        let mut state = self.start(begin.checked_sub(1).map(|i| text[i]))?;
        let mut longest = begin;
        for pos in begin..=text.len() {
            state = self.step(state, text.get(pos).copied());
            if self.matched(state) {
                longest = pos;
            }
            if self.dead(state) {
                break;
            }
        }
        Ok(longest)
    }
}
