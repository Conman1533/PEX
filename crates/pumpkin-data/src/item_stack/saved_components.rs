//! The `components` of a saved item stack as they were read, so PEX writes back what it doesn't
//! model, as D7-1 decided: a component PEX doesn't know or can't parse is kept as raw NBT
//! instead of dropping the whole stack, and a parsed component nobody changed is written in the
//! exact form it was saved in.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

#[derive(Debug, Default, PartialEq)]
pub struct SavedComponents {
    /// Components PEX doesn't know or couldn't parse, as saved, sorted by name.
    unparsed: Vec<(Box<str>, NbtTag)>,
    /// Each parsed component's saved tag and what PEX's writer made of it right after loading.
    parsed: Vec<(Box<str>, NbtTag, NbtTag)>,
}

impl SavedComponents {
    pub(super) fn keep_unparsed(&mut self, name: &str, tag: NbtTag) {
        self.unparsed.push((name.into(), tag));
    }

    pub(super) fn keep_parsed(&mut self, name: &str, saved: NbtTag, written: NbtTag) {
        if saved != written {
            self.parsed.push((name.into(), saved, written));
        }
    }

    /// `None` when nothing needs keeping, so a plain stack carries no extra data.
    pub(super) fn finish(mut self) -> Option<Self> {
        if self.unparsed.is_empty() && self.parsed.is_empty() {
            return None;
        }
        self.unparsed.sort_by(|a, b| a.0.cmp(&b.0));
        Some(self)
    }

    /// The tag to save for a parsed component: the one it was loaded from while PEX's writer
    /// still makes what it made on load, so the component is unchanged; otherwise `written`.
    pub(super) fn saved_form(&self, name: &str, written: NbtTag) -> NbtTag {
        self.parsed
            .iter()
            .find(|(saved_name, _, loaded)| saved_name.as_ref() == name && *loaded == written)
            .map_or(written, |(_, saved, _)| saved.clone())
    }

    /// Writes the unparsed components `components` doesn't already hold; a component PEX set
    /// since wins.
    pub(super) fn write_unparsed(&self, components: &mut NbtCompound) {
        for (name, tag) in &self.unparsed {
            let removal = name.strip_prefix('!').unwrap_or(name);
            if !components.has(removal) && !components.has(&format!("!{removal}")) {
                components.put(name, tag.clone());
            }
        }
    }

    /// Whether two stacks keep the same unparsed components, which decides if they stack.
    pub(super) fn same_unparsed(a: Option<&Self>, b: Option<&Self>) -> bool {
        let empty: &[(Box<str>, NbtTag)] = &[];
        a.map_or(empty, |s| &s.unparsed) == b.map_or(empty, |s| &s.unparsed)
    }
}

/// Warns once per component name, so a world full of one modded component logs one line.
pub(super) fn warn_unparsed(name: &str, reason: &str) {
    static WARNED: LazyLock<Mutex<HashSet<Box<str>>>> = LazyLock::new(Mutex::default);
    let first = WARNED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(name.into());
    if first {
        tracing::warn!("Keeping item component {name} unchanged: {reason}");
    }
}
