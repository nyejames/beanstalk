pub mod registry;
pub mod wasix_registry;

pub use registry::{
    HostFunctionDef, HostFunctionRegistry, ErrorHandling, RuntimeBackend,
    WasixFunctionDef, JsFunctionDef, RuntimeFunctionMapping,
    create_builtin_registry, create_builtin_registry_with_backend
};

pub use wasix_registry::{
    WasixFunctionDef, WasixFunctionRegistry,
    create_wasix_registry, WasixError, WasixContext,
    MemoryRegion, IOVec, WasixCallContext,
    WasixEnv, FdTable, ProcessInfo, WasixMemoryManager
};