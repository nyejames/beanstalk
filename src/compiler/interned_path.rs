use crate::compiler::string_interning::{StringId, StringTable};
use std::path::{Path, PathBuf};

/// An efficient path representation using interned string components.
///
/// InternedPath stores path components as a Vec<StringId>, allowing for:
/// - Memory-efficient storage when paths share common components
/// - Fast path operations (push, pop, parent, join) using vector operations
/// - Efficient comparison and hashing using StringId equality
/// - Conversion to/from standard PathBuf when needed for file system operations
///
/// This is particularly useful for scope tracking in the compiler where many
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

    /// Create an InternedPath from a PathBuf by interning each component
    pub fn from_path_buf(path: &Path, string_table: &mut StringTable) -> Self {
        let components = path
            .components()
            .filter_map(|component| {
                match component {
                    std::path::Component::Normal(os_str) => {
                        // Convert OsStr to string, handling potential UTF-8 issues
                        os_str.to_str().map(|s| string_table.intern(s))
                    }
                    std::path::Component::RootDir => {
                        // Represent root directory as empty string
                        Some(string_table.intern(""))
                    }
                    // Skip current dir (.) and parent dir (..) for now
                    // These could be handled specially if needed
                    _ => None,
                }
            })
            .collect();

        Self { components }
    }

    /// Create an InternedPath from a vector of StringIds
    pub fn from_components(components: Vec<StringId>) -> Self {
        Self { components }
    }

    /// Convert this InternedPath back to a PathBuf
    pub fn to_path_buf(&self, string_table: &StringTable) -> PathBuf {
        if self.components.is_empty() {
            return PathBuf::new();
        }

        let mut path = PathBuf::new();
        for &component_id in &self.components {
            let component_str = string_table.resolve(component_id);
            if component_str.is_empty() {
                // Empty string represents root directory
                path.push("/");
            } else {
                path.push(component_str);
            }
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

    pub fn join_header(&self, other: StringId, string_table: &mut StringTable) -> InternedPath {
        let mut new_components = self.components.clone();
        let other = format!("{}.header", string_table.resolve(other));
        let interned = string_table.get_or_intern(other);
        new_components.push(interned);
        InternedPath {
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
    pub fn file_name(&self) -> Option<StringId> {
        self.components.last().copied()
    }

    /// Get the last component as a string
    pub fn file_name_str<'a>(&self, string_table: &'a StringTable) -> Option<&'a str> {
        self.file_name().map(|id| string_table.resolve(id))
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

    /// Extract the simple name from a header path by removing the .header suffix
    ///
    /// For a path like "file.bst/function_name.header", returns StringId for "function_name"
    ///
    /// # Arguments
    /// * `string_table` - The string table for resolving and interning strings
    ///
    /// # Returns
    /// * `Some(StringId)` - The simple name without .header suffix
    /// * `None` - If the path is empty or doesn't end with .header
    ///
    /// # Examples
    /// ```
    /// // Path: "tests/cases/success/basic_function.bst/simple_function.header"
    /// // Returns: StringId for "simple_function"
    ///
    /// // Path: "file.bst/struct_name.header"
    /// // Returns: StringId for "struct_name"
    ///
    /// // Path: "file.bst/no_suffix"
    /// // Returns: Some(StringId for "no_suffix")
    ///
    /// // Path: "" (empty)
    /// // Returns: None
    /// ```
    pub fn extract_header_name(&self, string_table: &mut StringTable) -> Option<StringId> {
        // Get the last component (e.g., "function_name.header")
        let last_component_id = self.file_name()?;
        let last_component_str = string_table.resolve(last_component_id);

        // Remove the .header suffix if present
        if let Some(name_without_suffix) = last_component_str.strip_suffix(".header") {
            Some(string_table.get_or_intern(name_without_suffix.to_string()))
        } else {
            // If no .header suffix, return the component as-is
            Some(last_component_id)
        }
    }
}

impl Default for InternedPath {
    fn default() -> Self {
        Self::new()
    }
}
