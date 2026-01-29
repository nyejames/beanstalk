//! JS identifier mapping for codegen.
//!
//! HIR names are interned strings; this module sanitizes them into valid JS
//! identifiers, avoids reserved words, and ensures uniqueness (including
//! collisions with compiler-internal helper names).

use crate::compiler::string_interning::{InternedString, StringTable};
use std::collections::{HashMap, HashSet};

use super::formatting::sanitize_identifier;

pub struct JsIdentifierMap {
    map: HashMap<InternedString, String>,
    used: HashSet<String>,
}

impl JsIdentifierMap {
    pub fn new() -> Self {
        JsIdentifierMap {
            map: HashMap::new(),
            used: HashSet::new(),
        }
    }

    pub fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_owned());
    }

    pub fn get(&mut self, id: InternedString, table: &StringTable) -> String {
        if let Some(existing) = self.map.get(&id) {
            return existing.to_owned();
        }

        let raw = table.resolve(id);
        let mut candidate = sanitize_identifier(raw);

        if self.used.contains(&candidate) {
            let mut suffix = 1;
            let base = candidate.to_owned();
            loop {
                let next = format!("{}_{}", base, suffix);
                if !self.used.contains(&next) {
                    candidate = next;
                    break;
                }
                suffix += 1;
            }
        }

        self.used.insert(candidate.to_owned());
        self.map.insert(id, candidate.to_owned());
        candidate
    }
}
