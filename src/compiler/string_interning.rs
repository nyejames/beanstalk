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

/// Statistics about memory usage and performance of the string table
#[derive(Debug, Clone, Default)]
pub struct InterningStats {
    /// Total number of unique strings stored
    pub unique_strings: usize,
    /// Total number of intern() calls made
    pub total_intern_calls: usize,
    /// Number of times an existing string was found (cache hits)
    pub cache_hits: usize,
    /// Estimated memory saved by deduplication (in bytes)
    pub memory_saved: usize,
    /// Total memory used by the string table (in bytes)
    pub total_memory_used: usize,
}

impl InterningStats {
    /// Calculate the cache hit rate as a percentage
    pub fn cache_hit_rate(&self) -> f64 {
        if self.total_intern_calls == 0 {
            0.0
        } else {
            (self.cache_hits as f64 / self.total_intern_calls as f64) * 100.0
        }
    }
}

/// Debug information for tracking string usage patterns (only available in debug builds)
#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub struct DebugInfo {
    /// When this string was first interned
    pub first_interned_at: std::time::Instant,
    /// How many times this string has been interned
    pub intern_count: usize,
    /// Source locations where this string was interned (for debugging)
    pub source_locations: Vec<String>,
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
#[derive(Debug)]
pub struct StringTable {
    /// Primary storage: ID → String mapping for fast resolution
    strings: Vec<String>,
    
    /// Reverse lookup: String → ID mapping for fast interning
    string_to_id: HashMap<String, StringId>,
    
    /// Next available string ID
    next_id: u32,
    
    /// Statistics for memory usage and performance tracking
    stats: InterningStats,
    
    /// Debug information for development and analysis (debug builds only)
    #[cfg(debug_assertions)]
    debug_info: HashMap<StringId, DebugInfo>,
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
            stats: InterningStats::default(),
            #[cfg(debug_assertions)]
            debug_info: HashMap::new(),
        }
    }

    /// Intern a string slice, returning its unique ID.
    /// If the string already exists, returns the existing ID.
    /// If the string is new, stores it and returns a new ID.
    /// 
    /// Time complexity: O(1) average case
    pub fn intern(&mut self, s: &str) -> InternedString {
        self.stats.total_intern_calls += 1;

        // Check if we already have this string
        if let Some(&existing_id) = self.string_to_id.get(s) {
            self.stats.cache_hits += 1;
            
            // Update debug info for cache hit
            #[cfg(debug_assertions)]
            {
                if let Some(debug_info) = self.debug_info.get_mut(&existing_id) {
                    debug_info.intern_count += 1;
                }
            }
            
            return existing_id;
        }

        // String is new, so we need to store it
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Calculate memory savings (estimate)
        // Each duplicate string would have cost: String struct (24 bytes) + content
        // Now it costs: StringId (4 bytes)
        // So we save: 20 bytes + content length for each future duplicate
        let string_len = s.len();
        
        // Store the string and create the mapping
        self.strings.push(s.to_owned());
        self.string_to_id.insert(s.to_owned(), new_id);

        // Update statistics
        self.stats.unique_strings += 1;
        
        // Update memory usage calculation
        self.update_memory_stats();

        // Add debug information
        #[cfg(debug_assertions)]
        {
            self.debug_info.insert(new_id, DebugInfo {
                first_interned_at: std::time::Instant::now(),
                intern_count: 1,
                source_locations: Vec::new(),
            });
        }

        new_id
    }

    /// Resolve an interned string ID back to its string content.
    /// 
    /// Time complexity: O(1)
    /// 
    /// # Panics
    /// Panics if the StringId is invalid (not created by this StringTable)
    pub fn resolve(&self, id: InternedString) -> &str {
        self.strings.get(id.0 as usize)
            .map(|s| s.as_str())
            .unwrap_or_else(|| panic!("Invalid StringId: {}", id.0))
    }

    /// Efficiently intern a String by taking ownership, avoiding an extra allocation
    /// if the string is new. If the string already exists, the owned String is dropped
    /// and the existing ID is returned.
    /// 
    /// Time complexity: O(1) average case
    pub fn get_or_intern(&mut self, s: String) -> InternedString {
        self.stats.total_intern_calls += 1;

        // Check if we already have this string
        if let Some(&existing_id) = self.string_to_id.get(&s) {
            self.stats.cache_hits += 1;
            
            // Update debug info for cache hit
            #[cfg(debug_assertions)]
            {
                if let Some(debug_info) = self.debug_info.get_mut(&existing_id) {
                    debug_info.intern_count += 1;
                }
            }
            
            return existing_id;
        }

        // String is new, so we can use the owned String directly
        let new_id = StringId(self.next_id);
        self.next_id += 1;

        // Insert into reverse lookup first (we need to clone for the key)
        self.string_to_id.insert(s.clone(), new_id);
        
        // Then move the owned String into our storage
        self.strings.push(s);

        // Update statistics
        self.stats.unique_strings += 1;
        self.update_memory_stats();

        // Add debug information
        #[cfg(debug_assertions)]
        {
            self.debug_info.insert(new_id, DebugInfo {
                first_interned_at: std::time::Instant::now(),
                intern_count: 1,
                source_locations: Vec::new(),
            });
        }

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

    /// Update memory usage statistics based on current state
    fn update_memory_stats(&mut self) {
        // Calculate estimated memory saved
        // For each cache hit beyond the first, we saved approximately:
        // - String struct overhead (24 bytes on 64-bit systems)
        // - String content length
        let estimated_string_overhead = 24; // Size of String struct
        
        if self.stats.cache_hits > 0 {
            // Rough estimate: each cache hit saved the overhead plus average string length
            let avg_string_len = if self.strings.is_empty() {
                0
            } else {
                self.strings.iter().map(|s| s.len()).sum::<usize>() / self.strings.len()
            };
            
            self.stats.memory_saved = self.stats.cache_hits * (estimated_string_overhead + avg_string_len);
        }

        // Calculate total memory used
        let string_content: usize = self.strings.iter().map(|s| s.len()).sum();
        let vec_capacity = self.strings.capacity() * std::mem::size_of::<String>();
        let hashmap_capacity = self.string_to_id.capacity() * 
            (std::mem::size_of::<String>() + std::mem::size_of::<StringId>());
        
        self.stats.total_memory_used = string_content + vec_capacity + hashmap_capacity;
    }

    /// Get the current statistics for this string table
    pub fn stats(&self) -> &InterningStats {
        &self.stats
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
        let string_content_bytes: usize = self.strings.iter()
            .map(|s| s.len())
            .sum();
        
        let vec_overhead = self.strings.capacity() * std::mem::size_of::<String>();
        let hashmap_overhead = self.string_to_id.capacity() * 
            (std::mem::size_of::<String>() + std::mem::size_of::<StringId>());
        
        let total_bytes = string_content_bytes + vec_overhead + hashmap_overhead + 
            std::mem::size_of::<StringTable>();

        MemoryStats {
            total_bytes,
            string_content_bytes,
            overhead_bytes: total_bytes - string_content_bytes,
            unique_strings: self.len(),
            estimated_savings: self.stats.memory_saved,
        }
    }

    /// Get debug information for a specific string (debug builds only)
    #[cfg(debug_assertions)]
    pub fn debug_info(&self, id: StringId) -> Option<&DebugInfo> {
        self.debug_info.get(&id)
    }

    /// Dump all strings in the table for debugging purposes
    #[cfg(debug_assertions)]
    pub fn dump_strings(&self) -> Vec<(StringId, &str)> {
        self.strings.iter()
            .enumerate()
            .map(|(idx, s)| (StringId(idx as u32), s.as_str()))
            .collect()
    }

    /// Get the most frequently interned strings (debug builds only)
    #[cfg(debug_assertions)]
    pub fn most_frequent_strings(&self, limit: usize) -> Vec<(StringId, &str, usize)> {
        let mut strings_with_counts: Vec<_> = self.strings.iter()
            .enumerate()
            .map(|(idx, s)| {
                let id = StringId(idx as u32);
                let count = self.debug_info.get(&id)
                    .map(|info| info.intern_count)
                    .unwrap_or(1);
                (id, s.as_str(), count)
            })
            .collect();
        
        strings_with_counts.sort_by(|a, b| b.2.cmp(&a.2));
        strings_with_counts.truncate(limit);
        strings_with_counts
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
    /// Estimated memory saved through deduplication in bytes
    pub estimated_savings: usize,
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