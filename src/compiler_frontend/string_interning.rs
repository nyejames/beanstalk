use crate::projects::settings::MINIMUM_STRING_TABLE_CAPACITY;
use rustc_hash::FxHashMap;

/// A unique identifier for an interned string, represented as a u32 for memory efficiency.
/// This provides type safety to prevent mixing string IDs with other integer values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(u32);

impl StringId {
    /// Convert the StringId to its underlying u32 value for serialization
    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Create a StringId from a u32 value for deserialization
    #[inline]
    pub fn from_u32(id: u32) -> Self {
        Self(id)
    }

    /// Compare this interned string with a string slice efficiently without allocation.
    /// Requires access to the StringTable that created this ID.
    ///
    /// Time complexity: O(1) for ID resolution + O(n) for string comparison
    #[inline]
    pub fn eq_str(self, table: &StringTable, other: &str) -> bool {
        table.strings[self.0 as usize].as_ref() == other
    }

    /// Resolve this interned string using the provided StringTable.
    /// This is a convenience method that delegates to StringTable::resolve.
    ///
    /// Time complexity: O(1)
    #[inline]
    pub fn resolve<'a>(self, table: &'a StringTable) -> &'a str {
        table.resolve(self)
    }
}

/// Custom Debug implementation that shows the underlying ID value.
impl std::fmt::Display for StringId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StringId({})", self.0)
    }
}

/// A centralized string interning system that stores unique strings only once in memory.
///
/// The StringTable uses a dual-mapping approach for optimal performance:
/// - Vec<Box<str>> for O(1) ID→string resolution with minimal overhead
/// - FxHashMap<&str, StringId> for O(1) string→ID lookup during interning
#[derive(Debug, Clone)]
pub struct StringTable {
    /// Primary storage: ID → String mapping for fast resolution
    /// Using Box<str> instead of String saves 8 bytes per entry (no capacity field)
    strings: Vec<Box<str>>,

    /// Reverse lookup: String → ID mapping for fast interning
    /// FxHashMap is ~2-3x faster than default HashMap for string keys
    /// Stores borrowed references to avoid duplicating string data
    string_to_id: FxHashMap<&'static str, StringId>,

    /// Next available string ID
    next_id: u32,
}

impl StringTable {
    /// Create a new empty string table
    pub fn new() -> Self {
        Self {
            next_id: 0,
            strings: Vec::with_capacity(MINIMUM_STRING_TABLE_CAPACITY),
            string_to_id: FxHashMap::default(),
        }
    }

    /// Create a new string table with a specified initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            next_id: 0,
            strings: Vec::with_capacity(capacity + MINIMUM_STRING_TABLE_CAPACITY),
            string_to_id: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Intern a string slice, returning its unique ID.
    /// If the string already exists, returns the existing ID.
    /// If the string is new, stores it and returns a new ID.
    #[inline]
    pub fn intern(&mut self, s: &str) -> StringId {
        // Fast path: check if we already have this string
        // This is a single hash lookup with no allocations
        if let Some(&existing_id) = self.string_to_id.get(s) {
            return existing_id;
        }

        // Slow path: string is new, so we need to store it
        self.intern_new(s)
    }

    /// Internal helper for interning new strings (cold path)
    /// Marked as cold to optimize the hot path in `intern`
    #[cold]
    #[inline(never)]
    fn intern_new(&mut self, s: &str) -> StringId {
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Box<str> is more memory efficient than String (no capacity field)
        let boxed: Box<str> = s.into();

        // SAFETY: We're creating a 'static reference to data we own and will never drop
        // The string table owns the Box<str> for the program's lifetime
        // This is safe because:
        // 1. We never remove strings from the table
        // 2. We never reallocate the Box<str> (it's heap-allocated with stable address)
        // 3. The StringTable itself lives for the entire compilation
        let static_ref: &'static str =
            unsafe { std::mem::transmute::<&str, &'static str>(boxed.as_ref()) };

        // Insert into reverse lookup (uses the static ref as key - no allocation!)
        self.string_to_id.insert(static_ref, new_id);

        // Store the owned string
        self.strings.push(boxed);

        new_id
    }

    /// Resolve an interned string ID back to its string content.
    ///
    /// Time complexity: O(1)
    ///
    /// # Safety
    /// This uses unchecked indexing for maximum performance.
    /// StringIds are only created by this StringTable, so indices are guaranteed valid.
    #[inline]
    pub fn resolve(&self, id: StringId) -> &str {
        // SAFETY: StringIds are only created by this StringTable and are guaranteed
        // to be valid indices into self.strings
        unsafe { self.strings.get_unchecked(id.0 as usize).as_ref() }
    }

    /// Efficiently intern a String by taking ownership, avoiding an extra allocation
    /// if the string is new.
    ///
    /// Time complexity: O(1) average case
    #[inline]
    pub fn get_or_intern(&mut self, s: String) -> StringId {
        // Fast path: check if we already have this string
        if let Some(&existing_id) = self.string_to_id.get(s.as_str()) {
            return existing_id;
        }

        // Slow path: string is new
        self.intern_new_owned(s)
    }

    /// Internal helper for interning new owned strings (cold path)
    #[cold]
    #[inline(never)]
    fn intern_new_owned(&mut self, s: String) -> StringId {
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Convert directly to Box<str> to avoid keeping String's capacity overhead
        let boxed: Box<str> = s.into_boxed_str();

        // SAFETY: Same reasoning as intern_new
        let static_ref: &'static str =
            unsafe { std::mem::transmute::<&str, &'static str>(boxed.as_ref()) };

        self.string_to_id.insert(static_ref, new_id);
        self.strings.push(boxed);

        new_id
    }

    /// Try to resolve an interned string ID, returning None if the ID is invalid.
    ///
    /// Time complexity: O(1)
    #[inline]
    pub fn try_resolve(&self, id: StringId) -> Option<&str> {
        self.strings.get(id.0 as usize).map(|s| s.as_ref())
    }

    /// Check if a string is already interned without interning it.
    /// Returns the StringId if found, None otherwise.
    ///
    /// Time complexity: O(1) average case
    #[inline]
    pub fn get_existing(&self, s: &str) -> Option<StringId> {
        self.string_to_id.get(s).copied()
    }

    /// Get the number of unique strings stored in the table
    #[inline]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the string table is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Calculate detailed memory usage statistics
    pub fn memory_usage(&self) -> MemoryStats {
        let string_content_bytes: usize = self.strings.iter().map(|s| s.len()).sum();

        // Box<str> overhead: ptr + len (no capacity like String)
        let vec_overhead = self.strings.capacity() * std::mem::size_of::<Box<str>>();

        // FxHashMap overhead is minimal
        let hashmap_overhead = self.string_to_id.capacity()
            * (std::mem::size_of::<&str>() + std::mem::size_of::<StringId>());

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
            .map(|(idx, s)| (StringId(idx as u32), s.as_ref()))
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
