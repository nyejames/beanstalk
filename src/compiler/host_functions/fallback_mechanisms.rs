use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::tokens::TextLocation;

use std::collections::HashMap;

use super::wasix_registry::{WasixError, WasixFunctionDef};
use super::wasi_compatibility::WasiCompatibilityLayer;

/// Fallback mechanisms for WASI-only environments and runtime compatibility
#[derive(Debug, Clone)]
pub struct FallbackMechanisms {
    /// WASI compatibility layer for fallback logic
    compatibility_layer: WasiCompatibilityLayer,
    /// Fallback strategies for different runtime environments
    fallback_strategies: HashMap<RuntimeEnvironment, FallbackStrategy>,
    /// Whether to enable graceful degradation
    enable_graceful_degradation: bool,
    /// Whether to emit errors for missing WASIX support
    error_on_missing_wasix: bool,
    /// Detected runtime environment
    detected_environment: Option<RuntimeEnvironment>,
}

impl FallbackMechanisms {
    /// Create a new fallback mechanisms system
    pub fn new(compatibility_layer: WasiCompatibilityLayer) -> Self {
        let mut fallback = FallbackMechanisms {
            compatibility_layer,
            fallback_strategies: HashMap::new(),
            enable_graceful_degradation: true,
            error_on_missing_wasix: false,
            detected_environment: None,
        };

        // Initialize default fallback strategies
        fallback.initialize_default_strategies();
        fallback
    }

    /// Initialize default fallback strategies for different runtime environments
    fn initialize_default_strategies(&mut self) {
        // WASI-only environment: Use WASI imports with compatibility warnings
        self.fallback_strategies.insert(
            RuntimeEnvironment::WasiOnly,
            FallbackStrategy {
                strategy_type: FallbackType::WasiCompatibility,
                description: "Use WASI imports with compatibility layer".to_string(),
                supported_functions: vec![
                    "fd_write".to_string(),
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                ],
                unsupported_functions: vec![
                    "socket_create".to_string(),
                    "thread_spawn".to_string(),
                ],
                error_messages: HashMap::from([
                    ("socket_create".to_string(), "Networking not available in WASI-only environment".to_string()),
                    ("thread_spawn".to_string(), "Threading not available in WASI-only environment".to_string()),
                ]),
                suggestions: vec![
                    "Upgrade runtime to support WASIX for enhanced functionality".to_string(),
                    "Use single-threaded alternatives for threading operations".to_string(),
                ],
            },
        );

        // WASIX environment: Full support
        self.fallback_strategies.insert(
            RuntimeEnvironment::Wasix,
            FallbackStrategy {
                strategy_type: FallbackType::FullSupport,
                description: "Full WASIX support available".to_string(),
                supported_functions: vec![
                    "fd_write".to_string(),
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                    "socket_create".to_string(),
                    "thread_spawn".to_string(),
                ],
                unsupported_functions: vec![],
                error_messages: HashMap::new(),
                suggestions: vec![],
            },
        );

        // Mixed environment: Partial support with fallbacks
        self.fallback_strategies.insert(
            RuntimeEnvironment::Mixed,
            FallbackStrategy {
                strategy_type: FallbackType::PartialSupport,
                description: "Mixed WASI/WASIX support with runtime detection".to_string(),
                supported_functions: vec![
                    "fd_write".to_string(),
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                ],
                unsupported_functions: vec![],
                error_messages: HashMap::new(),
                suggestions: vec![
                    "Runtime will detect available features at execution time".to_string(),
                    "Graceful degradation enabled for unsupported features".to_string(),
                ],
            },
        );

        // Unknown environment: Conservative fallback
        self.fallback_strategies.insert(
            RuntimeEnvironment::Unknown,
            FallbackStrategy {
                strategy_type: FallbackType::Conservative,
                description: "Conservative fallback for unknown runtime".to_string(),
                supported_functions: vec![
                    "fd_write".to_string(),
                ],
                unsupported_functions: vec![
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                    "socket_create".to_string(),
                    "thread_spawn".to_string(),
                ],
                error_messages: HashMap::from([
                    ("fd_read".to_string(), "File reading may not be available".to_string()),
                    ("proc_exit".to_string(), "Process exit may not be available".to_string()),
                    ("socket_create".to_string(), "Networking not available".to_string()),
                    ("thread_spawn".to_string(), "Threading not available".to_string()),
                ]),
                suggestions: vec![
                    "Specify runtime environment for better compatibility".to_string(),
                    "Test functionality in target runtime environment".to_string(),
                ],
            },
        );
    }

    /// Detect the runtime environment based on available imports
    pub fn detect_runtime_environment(
        &mut self,
        available_modules: &[String],
    ) -> RuntimeEnvironment {
        let has_wasix = available_modules.iter().any(|m| m.starts_with("wasix"));
        let has_wasi = available_modules.iter().any(|m| m.contains("wasi"));

        let environment = if has_wasix {
            if has_wasi {
                RuntimeEnvironment::Mixed
            } else {
                RuntimeEnvironment::Wasix
            }
        } else if has_wasi {
            RuntimeEnvironment::WasiOnly
        } else {
            RuntimeEnvironment::Unknown
        };

        self.detected_environment = Some(environment.clone());
        environment
    }

    /// Get fallback strategy for a function in the current environment
    pub fn get_fallback_for_function(
        &self,
        function_name: &str,
        environment: &RuntimeEnvironment,
    ) -> Result<FunctionFallback, WasixError> {
        let strategy = self.fallback_strategies.get(environment)
            .ok_or_else(|| WasixError::configuration_error(
                "fallback_strategy",
                &format!("No strategy for environment: {:?}", environment),
                "Supported environment strategy",
                "Configure fallback strategy for this runtime environment",
            ))?;

        if strategy.supported_functions.contains(&function_name.to_string()) {
            Ok(FunctionFallback {
                function_name: function_name.to_string(),
                fallback_type: FunctionFallbackType::Supported,
                fallback_module: self.get_fallback_module(function_name, environment),
                fallback_function: function_name.to_string(),
                warning_message: None,
                error_message: None,
            })
        } else if strategy.unsupported_functions.contains(&function_name.to_string()) {
            let error_msg = strategy.error_messages.get(function_name);
            
            if self.enable_graceful_degradation {
                Ok(FunctionFallback {
                    function_name: function_name.to_string(),
                    fallback_type: FunctionFallbackType::GracefulDegradation,
                    fallback_module: "beanstalk_io".to_string(),
                    fallback_function: "noop".to_string(),
                    warning_message: Some(format!(
                        "Function '{}' not available in {:?} environment, using no-op fallback",
                        function_name, environment
                    )),
                    error_message: error_msg.cloned(),
                })
            } else {
                Ok(FunctionFallback {
                    function_name: function_name.to_string(),
                    fallback_type: FunctionFallbackType::Error,
                    fallback_module: "".to_string(),
                    fallback_function: "".to_string(),
                    warning_message: None,
                    error_message: error_msg.cloned().or_else(|| Some(format!(
                        "Function '{}' not supported in {:?} environment",
                        function_name, environment
                    ))),
                })
            }
        } else {
            // Try WASI compatibility migration
            if self.compatibility_layer.is_wasi_function(function_name) {
                match self.compatibility_layer.migrate_function_name(function_name) {
                    Ok(wasix_name) => Ok(FunctionFallback {
                        function_name: function_name.to_string(),
                        fallback_type: FunctionFallbackType::WasiMigration,
                        fallback_module: self.get_fallback_module(&wasix_name, environment),
                        fallback_function: wasix_name,
                        warning_message: Some(format!(
                            "WASI function '{}' migrated for {:?} environment",
                            function_name, environment
                        )),
                        error_message: None,
                    }),
                    Err(e) => Ok(FunctionFallback {
                        function_name: function_name.to_string(),
                        fallback_type: FunctionFallbackType::Error,
                        fallback_module: "".to_string(),
                        fallback_function: "".to_string(),
                        warning_message: None,
                        error_message: Some(format!(
                            "Cannot migrate WASI function '{}': {}",
                            function_name, e
                        )),
                    }),
                }
            } else {
                Ok(FunctionFallback {
                    function_name: function_name.to_string(),
                    fallback_type: FunctionFallbackType::Unknown,
                    fallback_module: "beanstalk_io".to_string(),
                    fallback_function: function_name.to_string(),
                    warning_message: Some(format!(
                        "Unknown function '{}', using default module",
                        function_name
                    )),
                    error_message: None,
                })
            }
        }
    }

    /// Get the appropriate module name for fallback in the given environment
    fn get_fallback_module(&self, function_name: &str, environment: &RuntimeEnvironment) -> String {
        match environment {
            RuntimeEnvironment::Wasix => "wasix_32v1".to_string(),
            RuntimeEnvironment::WasiOnly => {
                if self.compatibility_layer.is_wasi_function(function_name) {
                    "wasi_snapshot_preview1".to_string()
                } else {
                    "beanstalk_io".to_string()
                }
            }
            RuntimeEnvironment::Mixed => {
                // Prefer WASIX but fallback to WASI
                if self.compatibility_layer.is_wasi_function(function_name) {
                    "wasi_snapshot_preview1".to_string()
                } else {
                    "wasix_32v1".to_string()
                }
            }
            RuntimeEnvironment::Unknown => "beanstalk_io".to_string(),
        }
    }

    /// Check if a function is supported in the current environment
    pub fn is_function_supported(
        &self,
        function_name: &str,
        environment: &RuntimeEnvironment,
    ) -> bool {
        if let Some(strategy) = self.fallback_strategies.get(environment) {
            strategy.supported_functions.contains(&function_name.to_string())
        } else {
            false
        }
    }

    /// Generate error message for WASI-only runtime when WASIX features are used
    pub fn generate_wasi_only_error(
        &self,
        function_name: &str,
        location: &TextLocation,
    ) -> CompileError {
        CompileError {
            msg: format!(
                "WASIX function '{}' is not available in WASI-only runtime environment. \
                This function requires WASIX support for enhanced system interface features. \
                Consider upgrading your runtime to support WASIX or use alternative approaches.",
                function_name
            ),
            location: location.clone(),
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        }
    }

    /// Generate error message for missing WASIX support
    pub fn generate_missing_wasix_error(
        &self,
        function_name: &str,
        location: &TextLocation,
    ) -> CompileError {
        CompileError {
            msg: format!(
                "Function '{}' requires WASIX support which is not available in the current runtime. \
                WASIX provides enhanced WebAssembly system interfaces including networking, threading, and improved I/O. \
                Please ensure your runtime supports WASIX imports or configure fallback mechanisms.",
                function_name
            ),
            location: location.clone(),
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        }
    }

    /// Create a no-op fallback function definition
    pub fn create_noop_fallback(&self, original_function: &str) -> WasixFunctionDef {
        WasixFunctionDef::new(
            "beanstalk_io",
            "noop",
            vec![], // No parameters for no-op
            vec![], // No return value
            &format!("No-op fallback for unsupported function '{}'", original_function),
        )
    }

    /// Configure graceful degradation behavior
    pub fn set_graceful_degradation(&mut self, enable: bool) {
        self.enable_graceful_degradation = enable;
    }

    /// Configure error behavior for missing WASIX
    pub fn set_error_on_missing_wasix(&mut self, error: bool) {
        self.error_on_missing_wasix = error;
    }

    /// Get the detected runtime environment
    pub fn get_detected_environment(&self) -> Option<&RuntimeEnvironment> {
        self.detected_environment.as_ref()
    }

    /// Get fallback strategy for an environment
    pub fn get_strategy(&self, environment: &RuntimeEnvironment) -> Option<&FallbackStrategy> {
        self.fallback_strategies.get(environment)
    }

    /// Generate a compatibility report for the current environment
    pub fn generate_compatibility_report(&self) -> CompatibilityReport {
        let environment = self.detected_environment.clone()
            .unwrap_or(RuntimeEnvironment::Unknown);
        
        let strategy = self.fallback_strategies.get(&environment);
        
        CompatibilityReport {
            detected_environment: environment.clone(),
            strategy: strategy.cloned(),
            graceful_degradation_enabled: self.enable_graceful_degradation,
            error_on_missing_wasix: self.error_on_missing_wasix,
            recommendations: self.generate_environment_recommendations(&environment),
        }
    }

    /// Generate recommendations based on the detected environment
    fn generate_environment_recommendations(&self, environment: &RuntimeEnvironment) -> Vec<String> {
        match environment {
            RuntimeEnvironment::Wasix => vec![
                "✅ Full WASIX support detected - all features available".to_string(),
                "Consider using WASIX-specific features for enhanced functionality".to_string(),
            ],
            RuntimeEnvironment::WasiOnly => vec![
                "⚠️  WASI-only environment detected - limited functionality".to_string(),
                "Upgrade to WASIX-compatible runtime for networking and threading".to_string(),
                "Use compatibility layer for existing WASI code".to_string(),
            ],
            RuntimeEnvironment::Mixed => vec![
                "🔄 Mixed WASI/WASIX environment detected".to_string(),
                "Runtime feature detection will determine available functionality".to_string(),
                "Consider standardizing on WASIX for consistency".to_string(),
            ],
            RuntimeEnvironment::Unknown => vec![
                "❓ Unknown runtime environment - using conservative fallbacks".to_string(),
                "Specify runtime capabilities for better optimization".to_string(),
                "Test thoroughly in target deployment environment".to_string(),
            ],
        }
    }
}

/// Runtime environment types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuntimeEnvironment {
    /// WASI-only environment (no WASIX support)
    WasiOnly,
    /// Full WASIX environment
    Wasix,
    /// Mixed environment with both WASI and WASIX
    Mixed,
    /// Unknown or undetected environment
    Unknown,
}

impl std::fmt::Display for RuntimeEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeEnvironment::WasiOnly => write!(f, "WASI-only"),
            RuntimeEnvironment::Wasix => write!(f, "WASIX"),
            RuntimeEnvironment::Mixed => write!(f, "Mixed WASI/WASIX"),
            RuntimeEnvironment::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Fallback strategy for a runtime environment
#[derive(Debug, Clone)]
pub struct FallbackStrategy {
    /// Type of fallback strategy
    pub strategy_type: FallbackType,
    /// Human-readable description
    pub description: String,
    /// Functions supported in this environment
    pub supported_functions: Vec<String>,
    /// Functions not supported in this environment
    pub unsupported_functions: Vec<String>,
    /// Error messages for unsupported functions
    pub error_messages: HashMap<String, String>,
    /// General suggestions for this environment
    pub suggestions: Vec<String>,
}

/// Types of fallback strategies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackType {
    /// Full support for all features
    FullSupport,
    /// Partial support with some limitations
    PartialSupport,
    /// WASI compatibility mode
    WasiCompatibility,
    /// Conservative fallback for unknown environments
    Conservative,
}

/// Fallback information for a specific function
#[derive(Debug, Clone)]
pub struct FunctionFallback {
    /// Original function name
    pub function_name: String,
    /// Type of fallback applied
    pub fallback_type: FunctionFallbackType,
    /// Module to use for fallback
    pub fallback_module: String,
    /// Function name to use for fallback
    pub fallback_function: String,
    /// Warning message if applicable
    pub warning_message: Option<String>,
    /// Error message if fallback fails
    pub error_message: Option<String>,
}

/// Types of function fallbacks
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionFallbackType {
    /// Function is fully supported
    Supported,
    /// Function migrated from WASI
    WasiMigration,
    /// Graceful degradation (no-op or limited functionality)
    GracefulDegradation,
    /// Function not supported, will error
    Error,
    /// Unknown function, using default handling
    Unknown,
}

/// Compatibility report for the current environment
#[derive(Debug, Clone)]
pub struct CompatibilityReport {
    /// Detected runtime environment
    pub detected_environment: RuntimeEnvironment,
    /// Fallback strategy being used
    pub strategy: Option<FallbackStrategy>,
    /// Whether graceful degradation is enabled
    pub graceful_degradation_enabled: bool,
    /// Whether to error on missing WASIX
    pub error_on_missing_wasix: bool,
    /// Recommendations for this environment
    pub recommendations: Vec<String>,
}

impl CompatibilityReport {
    /// Format the compatibility report as a user-friendly message
    pub fn format_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("=== Runtime Compatibility Report ===\n\n");
        report.push_str(&format!("🔍 Detected Environment: {}\n", self.detected_environment));
        
        if let Some(strategy) = &self.strategy {
            report.push_str(&format!("📋 Strategy: {}\n", strategy.description));
            report.push_str(&format!("🔧 Fallback Type: {:?}\n", strategy.strategy_type));
            
            if !strategy.supported_functions.is_empty() {
                report.push_str(&format!("✅ Supported Functions: {}\n", 
                    strategy.supported_functions.join(", ")));
            }
            
            if !strategy.unsupported_functions.is_empty() {
                report.push_str(&format!("❌ Unsupported Functions: {}\n", 
                    strategy.unsupported_functions.join(", ")));
            }
        }
        
        report.push_str(&format!("🛡️  Graceful Degradation: {}\n", 
            if self.graceful_degradation_enabled { "Enabled" } else { "Disabled" }));
        report.push_str(&format!("🚨 Error on Missing WASIX: {}\n", 
            if self.error_on_missing_wasix { "Enabled" } else { "Disabled" }));
        
        if !self.recommendations.is_empty() {
            report.push_str("\n💡 Recommendations:\n");
            for (i, rec) in self.recommendations.iter().enumerate() {
                report.push_str(&format!("  {}. {}\n", i + 1, rec));
            }
        }
        
        report
    }
}

/// Create fallback mechanisms with the given compatibility layer
pub fn create_fallback_mechanisms(
    compatibility_layer: WasiCompatibilityLayer,
) -> FallbackMechanisms {
    FallbackMechanisms::new(compatibility_layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::host_functions::wasix_registry::create_wasix_registry;
    use crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer;

    fn create_test_fallback() -> FallbackMechanisms {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);
        FallbackMechanisms::new(compatibility_layer)
    }

    #[test]
    fn test_runtime_environment_detection() {
        let mut fallback = create_test_fallback();
        
        // Test WASIX detection
        let wasix_modules = vec!["wasix_32v1".to_string()];
        let env = fallback.detect_runtime_environment(&wasix_modules);
        assert_eq!(env, RuntimeEnvironment::Wasix);
        
        // Test WASI-only detection
        let wasi_modules = vec!["wasi_snapshot_preview1".to_string()];
        let env = fallback.detect_runtime_environment(&wasi_modules);
        assert_eq!(env, RuntimeEnvironment::WasiOnly);
        
        // Test mixed detection
        let mixed_modules = vec!["wasi_snapshot_preview1".to_string(), "wasix_32v1".to_string()];
        let env = fallback.detect_runtime_environment(&mixed_modules);
        assert_eq!(env, RuntimeEnvironment::Mixed);
    }

    #[test]
    fn test_function_fallback() {
        let fallback = create_test_fallback();
        
        // Test supported function in WASIX
        let fb = fallback.get_fallback_for_function("fd_write", &RuntimeEnvironment::Wasix)
            .expect("Failed to get fallback");
        assert_eq!(fb.fallback_type, FunctionFallbackType::Supported);
        
        // Test unsupported function in WASI-only with graceful degradation
        let fb = fallback.get_fallback_for_function("socket_create", &RuntimeEnvironment::WasiOnly)
            .expect("Failed to get fallback");
        assert_eq!(fb.fallback_type, FunctionFallbackType::GracefulDegradation);
    }

    #[test]
    fn test_compatibility_report() {
        let mut fallback = create_test_fallback();
        fallback.detect_runtime_environment(&vec!["wasix_32v1".to_string()]);
        
        let report = fallback.generate_compatibility_report();
        assert_eq!(report.detected_environment, RuntimeEnvironment::Wasix);
        assert!(report.strategy.is_some());
    }
}