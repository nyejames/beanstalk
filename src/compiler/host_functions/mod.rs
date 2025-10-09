pub mod registry;
pub mod wasix_registry;
pub mod wasi_compatibility;
pub mod migration_diagnostics;
pub mod fallback_mechanisms;

pub use registry::{
    HostFunctionDef, HostFunctionRegistry, ErrorHandling,
    create_builtin_registry
};

pub use wasix_registry::{
    WasixFunctionDef, WasixFunctionRegistry,
    create_wasix_registry, WasixError, WasixContext,
    MemoryRegion, IOVec, WasixCallContext,
    WasixEnv, FdTable, ProcessInfo, WasixMemoryManager
};

pub use wasi_compatibility::{
    WasiCompatibilityLayer, MigrationGuidance, MigrationType,
    create_wasi_compatibility_layer
};

pub use migration_diagnostics::{
    MigrationDiagnostics, WasiUsageDetection, MigrationWarning, 
    WarningSeverity, MigrationReport, create_migration_diagnostics
};

pub use fallback_mechanisms::{
    FallbackMechanisms, RuntimeEnvironment, FallbackStrategy, FallbackType,
    FunctionFallback, FunctionFallbackType, CompatibilityReport,
    create_fallback_mechanisms
};