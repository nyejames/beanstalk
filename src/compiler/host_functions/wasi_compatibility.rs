use crate::compiler::compiler_errors::CompileError;

use std::collections::HashMap;

use super::wasix_registry::{WasixError, WasixFunctionRegistry};

/// WASI compatibility layer for automatic migration to WASIX
#[derive(Debug, Clone)]
pub struct WasiCompatibilityLayer {
    /// Map from WASI function names to WASIX equivalents
    wasi_to_wasix_mapping: HashMap<String, String>,
    /// Map from WASI module names to WASIX equivalents
    wasi_module_mapping: HashMap<String, String>,
    /// Registry of WASIX functions for compatibility checking
    wasix_registry: WasixFunctionRegistry,
    /// Whether to emit warnings for WASI usage
    emit_warnings: bool,
}

impl WasiCompatibilityLayer {
    /// Create a new WASI compatibility layer
    pub fn new(wasix_registry: WasixFunctionRegistry) -> Self {
        let mut layer = WasiCompatibilityLayer {
            wasi_to_wasix_mapping: HashMap::new(),
            wasi_module_mapping: HashMap::new(),
            wasix_registry,
            emit_warnings: true,
        };

        // Initialize standard WASI to WASIX mappings
        layer.initialize_standard_mappings();
        layer
    }

    /// Initialize standard WASI to WASIX function and module mappings
    fn initialize_standard_mappings(&mut self) {
        // Module mappings - WASI modules to WASIX equivalents
        self.wasi_module_mapping.insert(
            "wasi_snapshot_preview1".to_string(),
            "wasix_32v1".to_string(),
        );
        self.wasi_module_mapping
            .insert("wasi_unstable".to_string(), "wasix_32v1".to_string());

        // Function mappings - WASI functions to WASIX equivalents
        // Most WASI functions have direct WASIX equivalents with same names
        self.wasi_to_wasix_mapping
            .insert("fd_write".to_string(), "fd_write".to_string());
        self.wasi_to_wasix_mapping
            .insert("fd_read".to_string(), "fd_read".to_string());
        self.wasi_to_wasix_mapping
            .insert("fd_close".to_string(), "fd_close".to_string());
        self.wasi_to_wasix_mapping
            .insert("fd_seek".to_string(), "fd_seek".to_string());
        self.wasi_to_wasix_mapping
            .insert("path_open".to_string(), "path_open".to_string());
        self.wasi_to_wasix_mapping
            .insert("environ_get".to_string(), "environ_get".to_string());
        self.wasi_to_wasix_mapping.insert(
            "environ_sizes_get".to_string(),
            "environ_sizes_get".to_string(),
        );
        self.wasi_to_wasix_mapping
            .insert("args_get".to_string(), "args_get".to_string());
        self.wasi_to_wasix_mapping
            .insert("args_sizes_get".to_string(), "args_sizes_get".to_string());
        self.wasi_to_wasix_mapping
            .insert("proc_exit".to_string(), "proc_exit".to_string());
        self.wasi_to_wasix_mapping
            .insert("random_get".to_string(), "random_get".to_string());
        self.wasi_to_wasix_mapping
            .insert("clock_time_get".to_string(), "clock_time_get".to_string());
    }

    /// Check if a module name is a WASI module that needs migration
    pub fn is_wasi_module(&self, module_name: &str) -> bool {
        self.wasi_module_mapping.contains_key(module_name)
    }

    /// Check if a function name is a WASI function that needs migration
    pub fn is_wasi_function(&self, function_name: &str) -> bool {
        self.wasi_to_wasix_mapping.contains_key(function_name)
    }

    /// Migrate a WASI module name to its WASIX equivalent
    pub fn migrate_module_name(&self, wasi_module: &str) -> Result<String, WasixError> {
        match self.wasi_module_mapping.get(wasi_module) {
            Some(wasix_module) => Ok(wasix_module.clone()),
            None => Err(WasixError::import_resolution_error(
                wasi_module,
                "unknown",
                &format!("WASI module '{}' is not supported", wasi_module),
                &format!("Use WASIX module 'wasix_32v1' instead of '{}'", wasi_module),
            )),
        }
    }

    /// Migrate a WASI function name to its WASIX equivalent
    pub fn migrate_function_name(&self, wasi_function: &str) -> Result<String, WasixError> {
        match self.wasi_to_wasix_mapping.get(wasi_function) {
            Some(wasix_function) => Ok(wasix_function.clone()),
            None => Err(WasixError::import_resolution_error(
                "wasi_snapshot_preview1",
                wasi_function,
                &format!("WASI function '{}' is not supported", wasi_function),
                &format!(
                    "Check if '{}' has a WASIX equivalent or use native Beanstalk functions",
                    wasi_function
                ),
            )),
        }
    }

    /// Generate migration guidance for a WASI import
    pub fn generate_migration_guidance(
        &self,
        wasi_module: &str,
        wasi_function: &str,
    ) -> Result<MigrationGuidance, WasixError> {
        let wasix_module = self.migrate_module_name(wasi_module)?;
        let wasix_function = self.migrate_function_name(wasi_function)?;

        let guidance = MigrationGuidance {
            original_module: wasi_module.to_string(),
            original_function: wasi_function.to_string(),
            target_module: wasix_module.clone(),
            target_function: wasix_function.clone(),
            migration_type: MigrationType::DirectMapping,
        };

        Ok(guidance)
    }

    /// Check if warnings are enabled
    pub fn should_emit_warnings(&self) -> bool {
        self.emit_warnings
    }

    /// Validate that the WASIX registry has all required functions for WASI compatibility
    pub fn validate_wasix_compatibility(&self) -> Result<(), WasixError> {
        let mut missing_functions = Vec::new();

        // Check that all mapped WASI functions have WASIX equivalents
        for (wasi_func, wasix_func) in &self.wasi_to_wasix_mapping {
            if !self.wasix_registry.has_function(wasix_func) {
                missing_functions.push(format!("{} -> {}", wasi_func, wasix_func));
            }
        }

        if !missing_functions.is_empty() {
            return Err(WasixError::configuration_error(
                "wasix_registry_completeness",
                &format!("Missing {} WASIX functions", missing_functions.len()),
                "Complete WASIX registry with all mapped functions",
                &format!(
                    "Add missing WASIX functions: {}",
                    missing_functions.join(", ")
                ),
            ));
        }

        Ok(())
    }
}

/// Migration guidance for converting WASI code to WASIX
#[derive(Debug, Clone)]
pub struct MigrationGuidance {
    /// Original WASI module name
    pub original_module: String,
    /// Original WASI function name
    pub original_function: String,
    /// Target WASIX module name
    pub target_module: String,
    /// Target WASIX function name
    pub target_function: String,
    /// Type of migration required
    pub migration_type: MigrationType,
}

impl MigrationGuidance {
    /// Format the migration guidance as a user-friendly message
    pub fn format_message(&self) -> String {
        let mut message = String::new();

        message.push_str(&format!(
            "Migration from WASI to WASIX:\n  {}:{} -> {}:{}\n",
            self.original_module, self.original_function, self.target_module, self.target_function
        ));

        message
    }
}

/// Type of migration required for WASI to WASIX conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationType {
    /// Direct 1:1 mapping between WASI and WASIX functions
    DirectMapping,
    /// Function behavior changes or enhancements
    BehaviorChange,
}

impl std::fmt::Display for MigrationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationType::DirectMapping => write!(f, "Direct Mapping"),
            MigrationType::BehaviorChange => write!(f, "Behavior Change"),
        }
    }
}

/// Create a WASI compatibility layer with the given WASIX registry
pub fn create_wasi_compatibility_layer(
    wasix_registry: WasixFunctionRegistry,
) -> Result<WasiCompatibilityLayer, CompileError> {
    let layer = WasiCompatibilityLayer::new(wasix_registry);

    // Validate that the compatibility layer is properly configured
    layer.validate_wasix_compatibility().map_err(|e| {
        CompileError::new_rule_error(
            format!("WASI compatibility validation failed: {}", e),
            crate::compiler::parsers::tokens::TextLocation::default(),
        )
    })?;

    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::host_functions::wasix_registry::create_wasix_registry;

    #[test]
    fn test_wasi_compatibility_layer_creation() {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);

        assert!(compatibility_layer.is_wasi_module("wasi_snapshot_preview1"));
        assert!(compatibility_layer.is_wasi_function("fd_write"));
        assert!(!compatibility_layer.is_wasi_module("wasix_32v1"));
    }

    #[test]
    fn test_module_migration() {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);

        let migrated = compatibility_layer
            .migrate_module_name("wasi_snapshot_preview1")
            .expect("Failed to migrate module name");
        assert_eq!(migrated, "wasix_32v1");
    }

    #[test]
    fn test_function_migration() {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);

        let migrated = compatibility_layer
            .migrate_function_name("fd_write")
            .expect("Failed to migrate function name");
        assert_eq!(migrated, "fd_write");
    }

    #[test]
    fn test_migration_guidance() {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);

        let guidance = compatibility_layer
            .generate_migration_guidance("wasi_snapshot_preview1", "fd_write")
            .expect("Failed to generate migration guidance");

        assert_eq!(guidance.original_module, "wasi_snapshot_preview1");
        assert_eq!(guidance.original_function, "fd_write");
        assert_eq!(guidance.target_module, "wasix_32v1");
        assert_eq!(guidance.target_function, "fd_write");
        assert_eq!(guidance.migration_type, MigrationType::DirectMapping);
    }

    #[test]
    fn test_unsupported_wasi_function() {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);

        let result = compatibility_layer.migrate_function_name("unsupported_function");
        assert!(result.is_err());
    }
}
