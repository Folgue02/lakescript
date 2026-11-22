//! Finding characters in strings by their position without walking them.
//!
//! A string is UTF-8, where a character takes 1 to 4 bytes, so the 5th
//! character isn't at a known place. Python makes every character of a string
//! the same size instead. Here the strings a script is working with are
//! remembered: one that's only ASCII needs nothing, since its characters are
//! its bytes, and for any other, where each character starts is written down
//! the first time it's needed. So `.length()` and `$text[$i]` are instant
//! from the second time on, a string that's never indexed costs nothing, and
//! the rest of the code keeps working on plain UTF-8 *(see Strings in
//! `ideas/language/builtin_types.md`)*.

use std::rc::Rc;

/// Shorter than this, walking the string is as fast as looking it up.
const SMALL: usize = 32;
/// How many strings are remembered. A loop works with a few at a time.
const REMEMBERED: usize = 8;

struct Entry {
    text: Rc<str>,
    /// How many characters it has.
    chars: usize,
    /// The byte where each character starts, or `None` when it's only ASCII.
    starts: Option<Vec<u32>>,
}

/// The strings indexed lately, the last one found first.
#[derive(Default)]
pub struct StringIndex {
    entries: Vec<Entry>,
}

impl StringIndex {
    /// How many characters a string has.
    pub fn len(&mut self, s: &Rc<str>) -> usize {
        match self.entry(s) {
            Some(entry) => entry.chars,
            None => s.chars().count(),
        }
    }

    /// The byte where the character at `position` starts, or the length of
    /// the string in bytes for a position at or after its end.
    pub fn byte_of(&mut self, s: &Rc<str>, position: usize) -> usize {
        match self.entry(s) {
            Some(Entry { starts: None, .. }) => position.min(s.len()),
            Some(Entry { starts: Some(starts), .. }) => starts.get(position).map(|b| *b as usize).unwrap_or(s.len()),
            None => s.char_indices().nth(position).map(|(b, _)| b).unwrap_or(s.len()),
        }
    }

    /// The position of the character that starts at `byte`.
    pub fn position_of(&mut self, s: &Rc<str>, byte: usize) -> usize {
        match self.entry(s) {
            Some(Entry { starts: None, .. }) => byte,
            Some(Entry { starts: Some(starts), .. }) => starts.partition_point(|b| (*b as usize) < byte),
            None => s[..byte].chars().count(),
        }
    }

    /// The character at `position`, as a string of its own.
    pub fn char_at<'a>(&mut self, s: &'a Rc<str>, position: usize) -> Option<&'a str> {
        let start = self.byte_of(s, position);
        let c = s[start..].chars().next()?;
        Some(&s[start..start + c.len_utf8()])
    }

    /// The characters from `start` up to `end`, by position.
    pub fn slice<'a>(&mut self, s: &'a Rc<str>, start: usize, end: usize) -> &'a str {
        let from = self.byte_of(s, start);
        let to = self.byte_of(s, end.max(start));
        &s[from..to]
    }

    /// What's known about a string, finding it out the first time. `None`
    /// for one so short, or so long, that it isn't worth remembering.
    fn entry(&mut self, s: &Rc<str>) -> Option<&Entry> {
        if s.len() < SMALL || s.len() > u32::MAX as usize {
            return None;
        }
        if let Some(i) = self.entries.iter().position(|e| Rc::ptr_eq(&e.text, s)) {
            // The one found is the first one looked at next time
            self.entries.swap(0, i);
            return self.entries.first();
        }
        // The ones only remembered here aren't used by the script anymore
        self.entries.retain(|e| Rc::strong_count(&e.text) > 1);
        self.entries.truncate(REMEMBERED - 1);
        let starts: Option<Vec<u32>> = if s.is_ascii() { None } else { Some(s.char_indices().map(|(b, _)| b as u32).collect()) };
        let chars = starts.as_ref().map(Vec::len).unwrap_or(s.len());
        self.entries.insert(0, Entry { text: s.clone(), chars, starts });
        self.entries.first()
    }
}
