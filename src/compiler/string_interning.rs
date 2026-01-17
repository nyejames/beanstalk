use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use std::collections::HashMap;

/// A unique identifier for an interned string, represented as a u32 for memory efficiency.
/// This provides type safety to prevent mixing string IDs with other integer values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(u32);

/// Type alias for better readability - InternedString is the same as StringId
pub type InternedString = StringId;

impl StringId {
    /// Convert the StringId to its underlying u32 value for serialization
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Create a StringId from a u32 value for deserialization
    pub fn from_u32(id: u32) -> Self {
        Self(id)
    }

    /// Compare this interned string with a string slice efficiently without allocation.
    /// Requires access to the StringTable that created this ID.
    ///
    /// Time complexity: O(1) for ID resolution + O(n) for string comparison
    pub fn eq_str(self, table: &StringTable, other: &str) -> bool {
        table.resolve(self) == other
    }

    /// Resolve this interned string using the provided StringTable.
    /// This is a convenience method that delegates to StringTable::resolve.
    ///
    /// Time complexity: O(1)
    pub fn resolve<'a>(self, table: &'a StringTable) -> &'a str {
        table.resolve(self)
    }
}

/// Custom Debug implementation that shows the underlying ID value.
/// Note: This doesn't show the actual string content since that requires
/// access to the StringTable. Use StringTable::resolve() for debugging
/// the actual string content.
impl std::fmt::Display for StringId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StringId({})", self.0)
    }
}

/// A centralized string interning system that stores unique strings only once in memory.
///
/// The StringTable uses a dual-mapping approach for optimal performance:
/// - Vec<String> for O(1) ID→string resolution
/// - HashMap<String, StringId> for O(1) string→ID lookup during interning
///
/// This design provides:
/// - O(1) interning operations (average case)
/// - O(1) string resolution by ID
/// - Memory deduplication for repeated strings
/// - Type-safe string IDs to prevent mixing with other integers
#[derive(Debug, Clone)]
pub struct StringTable {
    /// Primary storage: ID → String mapping for fast resolution
    strings: Vec<String>,

    /// Reverse lookup: String → ID mapping for fast interning
    string_to_id: HashMap<String, StringId>,

    /// Next available string ID
    next_id: u32,

    /// Stored source text locations for each interned string
    location: Vec<TextLocation>,
}

impl Default for StringTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StringTable {
    /// Create a new empty string table
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            string_to_id: HashMap::new(),
            next_id: 0,
            location: Vec::new(),
        }
    }

    /// Create a new string table with a specified initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: Vec::with_capacity(capacity),
            string_to_id: HashMap::with_capacity(capacity),
            next_id: 0,
            location: Vec::with_capacity(capacity),
        }
    }

    /// Intern a string slice, returning its unique ID.
    /// If the string already exists, returns the existing ID.
    /// If the string is new, stores it and returns a new ID.
    ///
    /// Time complexity: O(1) average case
    pub fn intern(&mut self, s: &str) -> InternedString {
        // Check if we already have this string
        if let Some(&existing_id) = self.string_to_id.get(s) {
            return existing_id;
        }

        // String is new, so we need to store it
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Store the string and create the mapping
        self.strings.push(s.to_owned());
        self.string_to_id.insert(s.to_owned(), new_id);

        new_id
    }

    /// Resolve an interned string ID back to its string content.
    ///
    /// Time complexity: O(1)
    ///
    /// # Panics
    /// Panics if the StringId is invalid (not created by this StringTable)
    pub fn resolve(&self, id: InternedString) -> &str {
        self.strings
            .get(id.0 as usize)
            .map(|s| s.as_str())
            .unwrap_or_else(|| panic!("Invalid StringId: {}", id.0))
    }

    /// Efficiently intern a String by taking ownership, avoiding an extra allocation
    /// if the string is new. If the string already exists, the owned String is dropped
    /// and the existing ID is returned.
    ///
    /// Time complexity: O(1) average case
    pub fn get_or_intern(&mut self, s: String) -> InternedString {
        // Check if we already have this string
        if let Some(&existing_id) = self.string_to_id.get(&s) {
            return existing_id;
        }

        // String is new, so we can use the owned String directly
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Insert into reverse lookup first (we need to clone for the key)
        self.string_to_id.insert(s.clone(), new_id);

        // Then move the owned String into our storage
        self.strings.push(s);

        new_id
    }

    /// Try to resolve an interned string ID, returning None if the ID is invalid
    /// instead of panicking.
    ///
    /// Time complexity: O(1)
    pub fn try_resolve(&self, id: InternedString) -> Option<&str> {
        self.strings.get(id.0 as usize).map(|s| s.as_str())
    }

    /// Check if a string is already interned without interning it.
    /// Returns the StringId if found, None otherwise.
    ///
    /// Time complexity: O(1) average case
    pub fn get_existing(&self, s: &str) -> Option<InternedString> {
        self.string_to_id.get(s).copied()
    }

    /// Get the number of unique strings stored in the table
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the string table is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Calculate detailed memory usage statistics
    pub fn memory_usage(&self) -> MemoryStats {
        let string_content_bytes: usize = self.strings.iter().map(|s| s.len()).sum();

        let vec_overhead = self.strings.capacity() * std::mem::size_of::<String>();
        let hashmap_overhead = self.string_to_id.capacity()
            * (std::mem::size_of::<String>() + std::mem::size_of::<StringId>());

        let total_bytes = string_content_bytes
            + vec_overhead
            + hashmap_overhead
            + std::mem::size_of::<StringTable>();

        MemoryStats {
            total_bytes,
            string_content_bytes,
            overhead_bytes: total_bytes - string_content_bytes,
            unique_strings: self.len(),
        }
    }

    /// Dump all strings in the table for debugging purposes
    #[cfg(debug_assertions)]
    pub fn dump_strings(&self) -> Vec<(StringId, &str)> {
        self.strings
            .iter()
            .enumerate()
            .map(|(idx, s)| (StringId(idx as u32), s.as_str()))
            .collect()
    }
}

/// Detailed memory usage statistics for the string table
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Total memory used by the string table in bytes
    pub total_bytes: usize,
    /// Memory used by actual string content in bytes
    pub string_content_bytes: usize,
    /// Memory used by data structure overhead in bytes
    pub overhead_bytes: usize,
    /// Number of unique strings stored
    pub unique_strings: usize,
}

impl MemoryStats {
    /// Calculate the overhead percentage
    pub fn overhead_percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.overhead_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Calculate the efficiency ratio (content vs total)
    pub fn efficiency_ratio(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.string_content_bytes as f64 / self.total_bytes as f64
        }
    }
}
