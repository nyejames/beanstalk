use crate::compiler_frontend::basic_utility_functions::is_valid_var_char;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use std::path::{Path, PathBuf};

/// An efficient path representation using interned string components.
///
/// InternedPath stores path components as a Vec<StringId>, allowing for:
/// - Memory-efficient storage when paths share common components
/// - Fast path operations (push, pop, parent, join) using vector operations
/// - Efficient comparison and hashing using StringId equality
/// - Conversion to/from standard PathBuf when needed for file system operations
///
/// This is particularly useful for scope tracking in the compiler_frontend where many
/// paths share common prefixes (like module names or directory structures).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InternedPath {
    /// Path components stored as interned string IDs
    /// Empty vector represents the root path
    components: Vec<StringId>,
}

impl InternedPath {
    /// Create a new empty path (equivalent to root)
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            components: Vec::with_capacity(capacity),
        }
    }

    /// Create an InternedPath from a PathBuf by interning each component
    pub fn from_path_buf(path: &Path, string_table: &mut StringTable) -> Self {
        let components = path
            .components()
            .filter_map(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .map(|s| string_table.intern(s))
            })
            .collect();

        Self { components }
    }

    /// Create an InternedPath from a vector of StringIds
    pub fn from_components(components: Vec<StringId>) -> Self {
        Self { components }
    }

    pub fn from_single_str(entry: &str, string_table: &mut StringTable) -> Self {
        let interned = string_table.intern(entry);
        Self {
            components: vec![interned],
        }
    }

    /// Convert this InternedPath back to a PathBuf
    pub fn to_path_buf(&self, string_table: &StringTable) -> PathBuf {
        if self.components.is_empty() {
            return PathBuf::new();
        }

        let mut path = PathBuf::new();
        for &component_id in &self.components {
            let component_str = string_table.resolve(component_id);
            path.push(component_str);
        }
        path
    }

    /// Push a new component to the end of this path
    pub fn push(&mut self, component: StringId) {
        self.components.push(component);
    }

    /// Push a string component to the end of this path (interns the string)
    pub fn push_str(&mut self, component: &str, string_table: &mut StringTable) {
        let component_id = string_table.intern(component);
        self.components.push(component_id);
    }

    /// Remove and return the last component of this path
    pub fn pop(&mut self) -> Option<StringId> {
        self.components.pop()
    }

    /// Get the parent path (all components except the last)
    /// Returns None if this is the root path
    pub fn parent(&self) -> Option<InternedPath> {
        if self.components.is_empty() {
            None
        } else {
            let mut parent_components = self.components.clone();
            parent_components.pop();
            Some(InternedPath {
                components: parent_components,
            })
        }
    }

    /// Join this path with another path
    pub fn join(&self, other: &InternedPath) -> InternedPath {
        let mut new_components = self.components.clone();
        new_components.extend_from_slice(&other.components);
        InternedPath {
            components: new_components,
        }
    }
    pub fn append(&self, new: StringId) -> Self {
        let mut new_components = self.components.clone();
        new_components.push(new);
        Self {
            components: new_components,
        }
    }

    /// Join this path with a string component (interns the string)
    pub fn join_str(&self, component: &str, string_table: &mut StringTable) -> InternedPath {
        let mut new_path = self.clone();
        new_path.push_str(component, string_table);
        new_path
    }

    /// Get the number of components in this path
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Check if this path is empty (root path)
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Get the last component of this path (the "file name")
    pub fn name(&self) -> Option<StringId> {
        self.components.last().copied()
    }

    /// Get the last component as a string
    pub fn name_str<'a>(&self, string_table: &'a StringTable) -> Option<&'a str> {
        self.name().map(|id| string_table.resolve(id))
    }

    /// Get an iterator over the components
    pub fn components(&self) -> impl Iterator<Item = StringId> + '_ {
        self.components.iter().copied()
    }

    /// Get the components as a slice
    pub fn as_components(&self) -> &[StringId] {
        &self.components
    }

    /// Check if this path starts with the given prefix path
    pub fn starts_with(&self, prefix: &InternedPath) -> bool {
        if prefix.components.len() > self.components.len() {
            return false;
        }

        self.components
            .iter()
            .zip(prefix.components.iter())
            .all(|(a, b)| a == b)
    }

    /// Check if this path ends with the given suffix path
    pub fn ends_with(&self, suffix: &InternedPath) -> bool {
        if suffix.components.len() > self.components.len() {
            return false;
        }

        let start_idx = self.components.len() - suffix.components.len();
        self.components[start_idx..]
            .iter()
            .zip(suffix.components.iter())
            .all(|(a, b)| a == b)
    }

    /// Create a relative path from this path to the target path
    /// Returns None if no relative path can be constructed
    pub fn relative_to(&self, base: &InternedPath) -> Option<InternedPath> {
        if !self.starts_with(base) {
            return None;
        }

        let relative_components = self.components[base.components.len()..].to_vec();
        Some(InternedPath {
            components: relative_components,
        })
    }

    /// Compare this path with a PathBuf efficiently
    /// Note: This creates a temporary StringTable for comparison, which is not ideal
    /// but necessary since we can't mutate the provided StringTable
    pub fn eq_path_buf(&self, other: &Path, string_table: &StringTable) -> bool {
        // For now, convert self to PathBuf and compare
        // This is less efficient but avoids the need to mutate string_table
        let self_path = self.to_path_buf(string_table);
        self_path == other
    }

    pub fn to_string(&self, string_table: &StringTable) -> String {
        self.to_path_buf(string_table).to_string_lossy().to_string()
    }

    pub fn to_interned_string(&self, string_table: &mut StringTable) -> StringId {
        let path_str = self.to_string(string_table);
        string_table.get_or_intern(path_str)
    }

    /// Extract the simple name from a header path by creating a name from the components,
    /// removing any invalid characters
    /// For a path like "file.bst/function_name.header", returns StringId for "file_function_name"
    pub fn extract_header_name(&self, string_table: &mut StringTable) -> StringId {
        // Combine each part of the path with underscores to create a unique name for the header
        let mut name = String::with_capacity(self.len() * 2);
        for component in self.components.iter() {
            let chars = string_table.resolve(*component).chars();

            // Strip any invalid characters from the component
            // Then add it to the string
            for c in chars {
                if is_valid_var_char(&c) {
                    name.push(c);
                } else {
                    name.push('_');
                }
            }
        }

        string_table.intern(&name)
    }
}

impl Default for InternedPath {
    fn default() -> Self {
        Self::new()
    }
}
