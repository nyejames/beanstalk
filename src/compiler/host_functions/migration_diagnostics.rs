use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_rule_error;

use super::wasi_compatibility::{MigrationGuidance, WasiCompatibilityLayer};
use super::wasix_registry::WasixError;

/// Migration diagnostics system for detecting and guiding WASI to WASIX transitions
#[derive(Debug, Clone)]
pub struct MigrationDiagnostics {
    /// WASI compatibility layer for migration logic
    compatibility_layer: WasiCompatibilityLayer,
    /// Detected WASI usage in the current compilation
    detected_wasi_usage: Vec<WasiUsageDetection>,
    /// Migration warnings to emit
    migration_warnings: Vec<MigrationWarning>,

    /// Whether to emit errors for unsupported WASI functions
    error_on_unsupported: bool,
}

impl MigrationDiagnostics {
    /// Create a new migration diagnostics system
    pub fn new(compatibility_layer: WasiCompatibilityLayer) -> Self {
        MigrationDiagnostics {
            compatibility_layer,
            detected_wasi_usage: Vec::new(),
            migration_warnings: Vec::new(),
            error_on_unsupported: false,
        }
    }

    /// Detect WASI function usage and record for migration guidance
    pub fn detect_wasi_usage(
        &mut self,
        function_name: &str,
        module_name: Option<&str>,
        location: &TextLocation,
    ) -> Result<Option<MigrationGuidance>, WasixError> {
        // Check if this is a WASI function
        if self.compatibility_layer.is_wasi_function(function_name) {
            let detection = WasiUsageDetection {
                function_name: function_name.to_string(),
                module_name: module_name.map(|s| s.to_string()),
                location: location.clone(),
                detection_type: WasiDetectionType::FunctionCall,
            };

            self.detected_wasi_usage.push(detection);

            // Generate migration guidance
            let module = module_name.unwrap_or("wasi_snapshot_preview1");
            let guidance = self
                .compatibility_layer
                .generate_migration_guidance(module, function_name)?;

            // Create migration warning
            let warning = MigrationWarning {
                message: format!("WASI function '{}' detected", function_name),
                location: location.clone(),
                guidance: guidance.clone(),
                severity: WarningSeverity::Info,
            };

            self.migration_warnings.push(warning);

            Ok(Some(guidance))
        } else {
            Ok(None)
        }
    }

    /// Check if a function is supported and provide error if not
    pub fn check_function_support(
        &self,
        function_name: &str,
        location: &TextLocation,
    ) -> Result<(), CompileError> {
        // Check if it's a WASI function that we can't migrate
        if self.compatibility_layer.is_wasi_function(function_name) {
            match self
                .compatibility_layer
                .migrate_function_name(function_name)
            {
                Ok(_) => Ok(()), // Migration available
                Err(e) if self.error_on_unsupported => {
                    return_rule_error!(
                        location.clone(),
                        "Unsupported WASI function '{}': {}. {}",
                        function_name,
                        e,
                        "Consider using native Beanstalk functions or update to WASIX-compatible alternatives"
                    );
                }
                Err(_) => Ok(()), // Allow with warning
            }
        } else {
            Ok(())
        }
    }
}

/// Detected WASI usage in the code
#[derive(Debug, Clone)]
pub struct WasiUsageDetection {
    /// Name of the WASI function used
    pub function_name: String,
    /// Module name if detected
    pub module_name: Option<String>,
    /// Location in source code
    pub location: TextLocation,
    /// Type of WASI usage detected
    pub detection_type: WasiDetectionType,
}

/// Type of WASI usage detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasiDetectionType {
    /// Direct function call to WASI function
    FunctionCall,
}

/// Migration warning with guidance
#[derive(Debug, Clone)]
pub struct MigrationWarning {
    /// Warning message
    pub message: String,
    /// Location in source code
    pub location: TextLocation,
    /// Migration guidance
    pub guidance: MigrationGuidance,
    /// Severity level
    pub severity: WarningSeverity,
}

/// Warning severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Informational message
    Info,
}

impl std::fmt::Display for WarningSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningSeverity::Info => write!(f, "INFO"),
        }
    }
}

/// Create a migration diagnostics system with the given compatibility layer
pub fn create_migration_diagnostics(
    compatibility_layer: WasiCompatibilityLayer,
) -> MigrationDiagnostics {
    MigrationDiagnostics::new(compatibility_layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer;
    use crate::compiler::host_functions::wasix_registry::create_wasix_registry;

    fn create_test_diagnostics() -> MigrationDiagnostics {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);
        MigrationDiagnostics::new(compatibility_layer)
    }

    #[test]
    fn test_wasi_function_detection() {
        use crate::compiler::parsers::tokens::CharPosition;
        use std::path::PathBuf;

        let mut diagnostics = create_test_diagnostics();
        let location = TextLocation::new(
            PathBuf::from("test.bs"),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        );

        let guidance = diagnostics
            .detect_wasi_usage("fd_write", Some("wasi_snapshot_preview1"), &location)
            .expect("Failed to detect WASI usage");

        assert!(guidance.is_some());
        assert_eq!(diagnostics.detected_wasi_usage.len(), 1);
        assert_eq!(diagnostics.migration_warnings.len(), 1);
    }
}
