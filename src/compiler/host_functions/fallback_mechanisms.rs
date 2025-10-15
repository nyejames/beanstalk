use std::collections::HashMap;

use super::wasi_compatibility::WasiCompatibilityLayer;
use super::wasix_registry::WasixError;

/// Fallback mechanisms for WASI-only environments and runtime compatibility
#[derive(Debug, Clone)]
pub struct FallbackMechanisms {
    /// WASI compatibility layer for fallback logic
    compatibility_layer: WasiCompatibilityLayer,
    /// Fallback strategies for different runtime environments
    fallback_strategies: HashMap<RuntimeEnvironment, FallbackStrategy>,
    /// Whether to enable graceful degradation
    enable_graceful_degradation: bool,

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
                    (
                        "socket_create".to_string(),
                        "Networking not available in WASI-only environment".to_string(),
                    ),
                    (
                        "thread_spawn".to_string(),
                        "Threading not available in WASI-only environment".to_string(),
                    ),
                ]),
            },
        );

        // WASIX environment: Full support
        self.fallback_strategies.insert(
            RuntimeEnvironment::Wasix,
            FallbackStrategy {
                supported_functions: vec![
                    "fd_write".to_string(),
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                    "socket_create".to_string(),
                    "thread_spawn".to_string(),
                ],
                unsupported_functions: vec![],
                error_messages: HashMap::new(),
            },
        );

        // Mixed environment: Partial support with fallbacks
        self.fallback_strategies.insert(
            RuntimeEnvironment::Mixed,
            FallbackStrategy {
                supported_functions: vec![
                    "fd_write".to_string(),
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                ],
                unsupported_functions: vec![],
                error_messages: HashMap::new(),
            },
        );

        // Unknown environment: Conservative fallback
        self.fallback_strategies.insert(
            RuntimeEnvironment::Unknown,
            FallbackStrategy {
                supported_functions: vec!["fd_write".to_string()],
                unsupported_functions: vec![
                    "fd_read".to_string(),
                    "proc_exit".to_string(),
                    "socket_create".to_string(),
                    "thread_spawn".to_string(),
                ],
                error_messages: HashMap::from([
                    (
                        "fd_read".to_string(),
                        "File reading may not be available".to_string(),
                    ),
                    (
                        "proc_exit".to_string(),
                        "Process exit may not be available".to_string(),
                    ),
                    (
                        "socket_create".to_string(),
                        "Networking not available".to_string(),
                    ),
                    (
                        "thread_spawn".to_string(),
                        "Threading not available".to_string(),
                    ),
                ]),
            },
        );
    }

    /// Get fallback strategy for a function in the current environment
    pub fn get_fallback_for_function(
        &self,
        function_name: &str,
        environment: &RuntimeEnvironment,
    ) -> Result<FunctionFallback, WasixError> {
        let strategy = self.fallback_strategies.get(environment).ok_or_else(|| {
            WasixError::configuration_error(
                "fallback_strategy",
                &format!("No strategy for environment: {:?}", environment),
                "Supported environment strategy",
                "Configure fallback strategy for this runtime environment",
            )
        })?;

        if strategy
            .supported_functions
            .contains(&function_name.to_string())
        {
            Ok(FunctionFallback {
                fallback_type: FunctionFallbackType::Supported,
                warning_message: None,
                error_message: None,
            })
        } else if strategy
            .unsupported_functions
            .contains(&function_name.to_string())
        {
            let error_msg = strategy.error_messages.get(function_name);

            if self.enable_graceful_degradation {
                Ok(FunctionFallback {
                    fallback_type: FunctionFallbackType::GracefulDegradation,
                    warning_message: Some(format!(
                        "Function '{}' not available in {:?} environment, using no-op fallback",
                        function_name, environment
                    )),
                    error_message: error_msg.cloned(),
                })
            } else {
                Ok(FunctionFallback {
                    fallback_type: FunctionFallbackType::Error,
                    warning_message: None,
                    error_message: error_msg.cloned().or_else(|| {
                        Some(format!(
                            "Function '{}' not supported in {:?} environment",
                            function_name, environment
                        ))
                    }),
                })
            }
        } else {
            // Try WASI compatibility migration
            if self.compatibility_layer.is_wasi_function(function_name) {
                match self
                    .compatibility_layer
                    .migrate_function_name(function_name)
                {
                    Ok(_wasix_name) => Ok(FunctionFallback {
                        fallback_type: FunctionFallbackType::WasiMigration,
                        warning_message: Some(format!(
                            "WASI function '{}' migrated for {:?} environment",
                            function_name, environment
                        )),
                        error_message: None,
                    }),
                    Err(e) => Ok(FunctionFallback {
                        fallback_type: FunctionFallbackType::Error,
                        warning_message: None,
                        error_message: Some(format!(
                            "Cannot migrate WASI function '{}': {}",
                            function_name, e
                        )),
                    }),
                }
            } else {
                Ok(FunctionFallback {
                    fallback_type: FunctionFallbackType::Unknown,
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

    /// Get the detected runtime environment
    pub fn get_detected_environment(&self) -> Option<&RuntimeEnvironment> {
        self.detected_environment.as_ref()
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
    /// Functions supported in this environment
    pub supported_functions: Vec<String>,
    /// Functions not supported in this environment
    pub unsupported_functions: Vec<String>,
    /// Error messages for unsupported functions
    pub error_messages: HashMap<String, String>,
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
    /// Type of fallback applied
    pub fallback_type: FunctionFallbackType,
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

/// Create fallback mechanisms with the given compatibility layer
pub fn create_fallback_mechanisms(
    compatibility_layer: WasiCompatibilityLayer,
) -> FallbackMechanisms {
    FallbackMechanisms::new(compatibility_layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer;
    use crate::compiler::host_functions::wasix_registry::create_wasix_registry;

    fn create_test_fallback() -> FallbackMechanisms {
        let wasix_registry = create_wasix_registry().expect("Failed to create WASIX registry");
        let compatibility_layer = WasiCompatibilityLayer::new(wasix_registry);
        FallbackMechanisms::new(compatibility_layer)
    }

    #[test]
    fn test_function_fallback() {
        let fallback = create_test_fallback();

        // Test supported function in WASIX
        let fb = fallback
            .get_fallback_for_function("fd_write", &RuntimeEnvironment::Wasix)
            .expect("Failed to get fallback");
        assert_eq!(fb.fallback_type, FunctionFallbackType::Supported);

        // Test unsupported function in WASI-only with graceful degradation
        let fb = fallback
            .get_fallback_for_function("socket_create", &RuntimeEnvironment::WasiOnly)
            .expect("Failed to get fallback");
        assert_eq!(fb.fallback_type, FunctionFallbackType::GracefulDegradation);
    }
}
