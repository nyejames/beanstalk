//! Declarative project config-key registry.
//!
//! WHAT: describes every key name that Stage 0 config loading is allowed to accept,
//!       who owns it (`Core` or `Backend`), and the broad value shape it expects.
//! WHY: prevents unknown config keys from being silently stored in `Config.settings`,
//!       and lets Stage 0 decide whether a folded value belongs in a typed `Config`
//!       field or in the backend-owned settings map.

/// Who is responsible for the semantic meaning and validation of a config key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfigKeyOwner {
    /// Owned by the compiler core / build system. Stage 0 applies these directly.
    Core,
    /// Owned by the backend builder. Stage 0 stores the raw string in `Config.settings`
    /// and `BackendBuilder::validate_project_config` enforces semantics.
    Backend,
}

/// Broad value category for a config key.
///
/// WHY: gives Stage 0 enough information to reject clearly wrong shapes early
/// without duplicating backend-specific parsing logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfigValueShape {
    /// A single string value (string literal or folded template).
    String,
    /// A collection of string values, or a single string treated as a one-element collection.
    StringCollection,
    /// A boolean literal (`true` or `false`).
    Bool,
    /// A string value that must belong to a fixed closed set of allowed strings.
    ClosedStringSet { allowed: &'static [&'static str] },
}

/// Human-readable name for a config value shape, used in diagnostics.
///
/// NOTE: `ClosedStringSet` carries its allowed values inline, so callers that need
/// a rendered name for that variant should format the allowed list locally.
pub fn config_value_shape_name(shape: ConfigValueShape) -> &'static str {
    match shape {
        ConfigValueShape::String => "a string value",
        ConfigValueShape::StringCollection => "a collection of strings",
        ConfigValueShape::Bool => "a boolean value",
        ConfigValueShape::ClosedStringSet { .. } => "one of the allowed values",
    }
}

/// One entry in the config-key registry.
#[derive(Clone, Debug)]
pub struct ConfigKeyEntry {
    pub name: &'static str,
    pub owner: ConfigKeyOwner,
    pub shape: ConfigValueShape,
}

/// The complete set of config keys accepted for a given project build.
///
/// WHAT: union of core keys (always present) and backend-declared keys.
/// WHY: Stage 0 config loading receives this from the selected builder so
/// unknown-key rejection is consistent with the actual backend surface.
#[derive(Clone, Debug, Default)]
pub struct ProjectConfigKeyRegistry {
    entries: Vec<ConfigKeyEntry>,
}

impl ProjectConfigKeyRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Register a single allowed config key.
    pub fn register(&mut self, name: &'static str, owner: ConfigKeyOwner, shape: ConfigValueShape) {
        self.entries.push(ConfigKeyEntry { name, owner, shape });
    }

    /// Register a core-owned string key.
    pub fn register_core_string(&mut self, name: &'static str) {
        self.register(name, ConfigKeyOwner::Core, ConfigValueShape::String);
    }

    /// Register a core-owned string-collection key.
    pub fn register_core_string_collection(&mut self, name: &'static str) {
        self.register(
            name,
            ConfigKeyOwner::Core,
            ConfigValueShape::StringCollection,
        );
    }

    /// Register a core-owned closed-string-set key.
    pub fn register_core_closed_string_set(
        &mut self,
        name: &'static str,
        allowed: &'static [&'static str],
    ) {
        self.register(
            name,
            ConfigKeyOwner::Core,
            ConfigValueShape::ClosedStringSet { allowed },
        );
    }

    /// Register a backend-owned string key.
    pub fn register_backend_string(&mut self, name: &'static str) {
        self.register(name, ConfigKeyOwner::Backend, ConfigValueShape::String);
    }

    /// Register a backend-owned boolean key.
    pub fn register_backend_bool(&mut self, name: &'static str) {
        self.register(name, ConfigKeyOwner::Backend, ConfigValueShape::Bool);
    }

    /// Register a backend-owned closed-string-set key.
    pub fn register_backend_closed_string_set(
        &mut self,
        name: &'static str,
        allowed: &'static [&'static str],
    ) {
        self.register(
            name,
            ConfigKeyOwner::Backend,
            ConfigValueShape::ClosedStringSet { allowed },
        );
    }

    /// Look up a key by its source-level name.
    pub fn lookup(&self, name: &str) -> Option<&ConfigKeyEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Returns `true` if the given name is a known config key.
    pub fn is_known(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Iterate over all registered entries.
    pub fn entries(&self) -> &[ConfigKeyEntry] {
        &self.entries
    }
}
