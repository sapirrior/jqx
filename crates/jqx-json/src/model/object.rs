//! Insertion-ordered JSON object representation.
//!
//! Employs a linear scan over key-value pairs for common small objects (<=16 keys)
//! to maintain cache locality and prevent `HashDoS` vulnerabilities.

use super::value::Value;
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq)]
pub struct Object<'a> {
    pub(crate) entries: Vec<(Cow<'a, str>, Value<'a>)>,
}

impl<'a> Object<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, key: Cow<'a, str>, value: Value<'a>) {
        for (k, v) in &mut self.entries {
            if k == &key {
                *v = value;
                return;
            }
        }
        self.entries.push((key, value));
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value<'a>> {
        for (k, v) in self.entries.iter().rev() {
            if k.as_ref() == key {
                return Some(v);
            }
        }
        None
    }

    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Cow<'a, str>, &Value<'a>)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Cow<'a, str>, &mut Value<'a>)> {
        self.entries.iter_mut().map(|(k, v)| (&*k, v))
    }
}

impl Default for Object<'_> {
    fn default() -> Self {
        Self::new()
    }
}
