use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_rule_error;
use std::collections::HashMap;

use super::wasi_compatibility::{WasiCompatibilityLayer, MigrationGuidance, MigrationType};
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
    /// Whether to emit warnings for WASIX-specific features
    warn_wasix_features: bool,
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
            warn_wasix_features: true,
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
            let guidance = self.compatibility_layer.generate_migration_guidance(module, function_name)?;
            
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

    /// Detect WASI module imports and record for migration guidance
    pub fn detect_wasi_module_import(
        &mut self,
        module_name: &str,
        location: &TextLocation,
    ) -> Result<Option<MigrationGuidance>, WasixError> {
        // Check if this is a WASI module
        if self.compatibility_layer.is_wasi_module(module_name) {
            let detection = WasiUsageDetection {
                function_name: "module_import".to_string(),
                module_name: Some(module_name.to_string()),
                location: location.clone(),
                detection_type: WasiDetectionType::ModuleImport,
            };

            self.detected_wasi_usage.push(detection);

            // Generate migration guidance for the module
            let guidance = MigrationGuidance {
                original_module: module_name.to_string(),
                original_function: "module_import".to_string(),
                target_module: self.compatibility_layer.migrate_module_name(module_name)?,
                target_function: "module_import".to_string(),
                migration_type: MigrationType::DirectMapping,
                warnings: vec![
                    format!("WASI module '{}' is deprecated", module_name),
                    "Consider using WASIX for enhanced functionality".to_string(),
                ],
                suggestions: vec![
                    format!("Replace '{}' imports with 'wasix_32v1'", module_name),
                    "Update runtime to support WASIX".to_string(),
                ],
                compatibility_notes: vec![
                    "WASIX provides backward compatibility with WASI".to_string(),
                    "Enhanced error reporting and additional features available".to_string(),
                ],
            };

            // Create migration warning
            let warning = MigrationWarning {
                message: format!("WASI module '{}' import detected", module_name),
                location: location.clone(),
                guidance: guidance.clone(),
                severity: WarningSeverity::Warning,
            };

            self.migration_warnings.push(warning);

            Ok(Some(guidance))
        } else {
            Ok(None)
        }
    }

    /// Detect WASIX-specific feature usage and warn about compatibility
    pub fn detect_wasix_feature_usage(
        &mut self,
        feature_name: &str,
        location: &TextLocation,
    ) {
        if self.warn_wasix_features {
            let warning = MigrationWarning {
                message: format!("WASIX-specific feature '{}' used", feature_name),
                location: location.clone(),
                guidance: MigrationGuidance {
                    original_module: "wasix_32v1".to_string(),
                    original_function: feature_name.to_string(),
                    target_module: "wasix_32v1".to_string(),
                    target_function: feature_name.to_string(),
                    migration_type: MigrationType::BehaviorChange,
                    warnings: vec![
                        format!("Feature '{}' is WASIX-specific and not available in WASI", feature_name),
                        "This code will not work on WASI-only runtimes".to_string(),
                    ],
                    suggestions: vec![
                        "Ensure your runtime supports WASIX".to_string(),
                        "Consider providing fallback for WASI-only environments".to_string(),
                    ],
                    compatibility_notes: vec![
                        "WASIX features provide enhanced functionality".to_string(),
                        "Graceful degradation recommended for broader compatibility".to_string(),
                    ],
                },
                severity: WarningSeverity::Info,
            };

            self.migration_warnings.push(warning);
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
            match self.compatibility_layer.migrate_function_name(function_name) {
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

    /// Get all detected WASI usage
    pub fn get_detected_wasi_usage(&self) -> &[WasiUsageDetection] {
        &self.detected_wasi_usage
    }

    /// Get all migration warnings
    pub fn get_migration_warnings(&self) -> &[MigrationWarning] {
        &self.migration_warnings
    }

    /// Generate a comprehensive migration report
    pub fn generate_migration_report(&self) -> MigrationReport {
        let mut function_usage = HashMap::new();
        let mut module_usage = HashMap::new();

        // Categorize detected usage
        for detection in &self.detected_wasi_usage {
            match detection.detection_type {
                WasiDetectionType::FunctionCall => {
                    *function_usage.entry(detection.function_name.clone()).or_insert(0) += 1;
                }
                WasiDetectionType::ModuleImport => {
                    if let Some(ref module) = detection.module_name {
                        *module_usage.entry(module.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        MigrationReport {
            total_wasi_detections: self.detected_wasi_usage.len(),
            function_usage,
            module_usage,
            warnings: self.migration_warnings.clone(),
            migration_recommendations: self.generate_migration_recommendations(),
        }
    }

    /// Generate migration recommendations based on detected usage
    fn generate_migration_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        if !self.detected_wasi_usage.is_empty() {
            recommendations.push("Consider migrating from WASI to WASIX for enhanced functionality".to_string());
            recommendations.push("Update your runtime to support WASIX imports".to_string());
        }

        // Check for specific patterns
        let has_fd_write = self.detected_wasi_usage.iter()
            .any(|d| d.function_name == "fd_write");
        if has_fd_write {
            recommendations.push("Use native Beanstalk print() function instead of direct fd_write calls".to_string());
        }

        let has_wasi_modules = self.detected_wasi_usage.iter()
            .any(|d| d.detection_type == WasiDetectionType::ModuleImport);
        if has_wasi_modules {
            recommendations.push("Replace WASI module imports with 'wasix_32v1' for better compatibility".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("No WASI usage detected - you're already using modern interfaces!".to_string());
        }

        recommendations
    }

    /// Configure warning behavior
    pub fn set_warn_wasix_features(&mut self, warn: bool) {
        self.warn_wasix_features = warn;
    }

    /// Configure error behavior for unsupported functions
    pub fn set_error_on_unsupported(&mut self, error: bool) {
        self.error_on_unsupported = error;
    }

    /// Clear all detected usage and warnings (for new compilation)
    pub fn clear(&mut self) {
        self.detected_wasi_usage.clear();
        self.migration_warnings.clear();
    }

    /// Format all migration warnings as user-friendly messages
    pub fn format_warnings(&self) -> Vec<String> {
        self.migration_warnings.iter()
            .map(|w| w.format_message())
            .collect()
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
    /// Import of WASI module
    ModuleImport,
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

impl MigrationWarning {
    /// Format the warning as a user-friendly message
    pub fn format_message(&self) -> String {
        format!(
            "{} at {}:{}: {}\n{}",
            self.severity,
            self.location.start_pos.line_number,
            self.location.start_pos.char_column,
            self.message,
            self.guidance.format_message()
        )
    }
}

/// Warning severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Informational message
    Info,
    /// Warning that should be addressed
    Warning,
    /// Error that must be fixed
    Error,
}

impl std::fmt::Display for WarningSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningSeverity::Info => write!(f, "INFO"),
            WarningSeverity::Warning => write!(f, "WARNING"),
            WarningSeverity::Error => write!(f, "ERROR"),
        }
    }
}

/// Comprehensive migration report
#[derive(Debug, Clone)]
pub struct MigrationReport {
    /// Total number of WASI detections
    pub total_wasi_detections: usize,
    /// Function usage counts
    pub function_usage: HashMap<String, usize>,
    /// Module usage counts
    pub module_usage: HashMap<String, usize>,
    /// All migration warnings
    pub warnings: Vec<MigrationWarning>,
    /// Migration recommendations
    pub migration_recommendations: Vec<String>,
}

impl MigrationReport {
    /// Format the report as a user-friendly message
    pub fn format_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== WASI to WASIX Migration Report ===\n\n");

        if self.total_wasi_detections == 0 {
            report.push_str("✅ No WASI usage detected - your code is already using modern interfaces!\n");
            return report;
        }

        report.push_str(&format!("📊 Total WASI usage detected: {}\n\n", self.total_wasi_detections));

        if !self.function_usage.is_empty() {
            report.push_str("🔧 WASI Functions Used:\n");
            for (func, count) in &self.function_usage {
                report.push_str(&format!("  - {}: {} occurrence(s)\n", func, count));
            }
            report.push('\n');
        }

        if !self.module_usage.is_empty() {
            report.push_str("📦 WASI Modules Imported:\n");
            for (module, count) in &self.module_usage {
                report.push_str(&format!("  - {}: {} occurrence(s)\n", module, count));
            }
            report.push('\n');
        }

        if !self.migration_recommendations.is_empty() {
            report.push_str("💡 Migration Recommendations:\n");
            for (i, rec) in self.migration_recommendations.iter().enumerate() {
                report.push_str(&format!("  {}. {}\n", i + 1, rec));
            }
            report.push('\n');
        }

        if !self.warnings.is_empty() {
            report.push_str("⚠️  Detailed Warnings:\n");
            for warning in &self.warnings {
                report.push_str(&format!("  {}\n", warning.format_message()));
            }
        }

        report
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
    use crate::compiler::host_functions::wasix_registry::create_wasix_registry;
    use crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer;

    fn create_test_diagnostics() -> MigrationDiagnostics {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);
        MigrationDiagnostics::new(compatibility_layer)
    }

    #[test]
    fn test_wasi_function_detection() {
        use std::path::PathBuf;
        use crate::compiler::parsers::tokens::CharPosition;
        
        let mut diagnostics = create_test_diagnostics();
        let location = TextLocation::new(
            PathBuf::from("test.bs"),
            CharPosition { line_number: 1, char_column: 1 },
            CharPosition { line_number: 1, char_column: 1 }
        );

        let guidance = diagnostics.detect_wasi_usage("fd_write", Some("wasi_snapshot_preview1"), &location)
            .expect("Failed to detect WASI usage");

        assert!(guidance.is_some());
        assert_eq!(diagnostics.get_detected_wasi_usage().len(), 1);
        assert_eq!(diagnostics.get_migration_warnings().len(), 1);
    }

    #[test]
    fn test_wasi_module_detection() {
        use std::path::PathBuf;
        use crate::compiler::parsers::tokens::CharPosition;
        
        let mut diagnostics = create_test_diagnostics();
        let location = TextLocation::new(
            PathBuf::from("test.bs"),
            CharPosition { line_number: 1, char_column: 1 },
            CharPosition { line_number: 1, char_column: 1 }
        );

        let guidance = diagnostics.detect_wasi_module_import("wasi_snapshot_preview1", &location)
            .expect("Failed to detect WASI module");

        assert!(guidance.is_some());
        assert_eq!(diagnostics.get_detected_wasi_usage().len(), 1);
        assert_eq!(diagnostics.get_migration_warnings().len(), 1);
    }

    #[test]
    fn test_migration_report_generation() {
        use std::path::PathBuf;
        use crate::compiler::parsers::tokens::CharPosition;
        
        let mut diagnostics = create_test_diagnostics();
        let location = TextLocation::new(
            PathBuf::from("test.bs"),
            CharPosition { line_number: 1, char_column: 1 },
            CharPosition { line_number: 1, char_column: 1 }
        );

        // Add some detections
        diagnostics.detect_wasi_usage("fd_write", Some("wasi_snapshot_preview1"), &location).unwrap();
        diagnostics.detect_wasi_module_import("wasi_snapshot_preview1", &location).unwrap();

        let report = diagnostics.generate_migration_report();
        assert_eq!(report.total_wasi_detections, 2);
        assert!(report.function_usage.contains_key("fd_write"));
        assert!(report.module_usage.contains_key("wasi_snapshot_preview1"));
    }

    #[test]
    fn test_wasix_feature_detection() {
        use std::path::PathBuf;
        use crate::compiler::parsers::tokens::CharPosition;
        
        let mut diagnostics = create_test_diagnostics();
        let location = TextLocation::new(
            PathBuf::from("test.bs"),
            CharPosition { line_number: 1, char_column: 1 },
            CharPosition { line_number: 1, char_column: 1 }
        );

        diagnostics.detect_wasix_feature_usage("socket_create", &location);

        assert_eq!(diagnostics.get_migration_warnings().len(), 1);
        let warning = &diagnostics.get_migration_warnings()[0];
        assert_eq!(warning.severity, WarningSeverity::Info);
    }
}