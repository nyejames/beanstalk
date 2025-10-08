// Embedding support for running Beanstalk in other applications
//
// Provides APIs for embedding Beanstalk runtime in Rust applications
// with support for hot reloading and custom IO integration.

use crate::compiler::compiler_errors::CompileError;
use crate::runtime::{RuntimeConfig, BeanstalkRuntime};
use wasmer::{Store, Module, Instance, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Embedded runtime for running Beanstalk code within Rust applications
pub struct EmbeddedRuntime {
    runtime: BeanstalkRuntime,
    store: Arc<Mutex<Store>>,
    loaded_modules: Arc<Mutex<HashMap<String, Module>>>,
    hot_reload_enabled: bool,
}

impl EmbeddedRuntime {
    /// Create a new embedded runtime
    pub fn new(config: &RuntimeConfig) -> Result<Self, CompileError> {
        let runtime = BeanstalkRuntime::new(config.clone());
        let store = Arc::new(Mutex::new(Store::default()));
        let loaded_modules = Arc::new(Mutex::new(HashMap::new()));
        
        Ok(Self {
            runtime,
            store,
            loaded_modules,
            hot_reload_enabled: config.hot_reload,
        })
    }

    /// Load a Beanstalk module from WASM bytes
    pub fn load_module(&self, module_name: &str, wasm_bytes: &[u8]) -> Result<(), CompileError> {
        let store_guard = self.store.lock().unwrap();
        let module = Module::new(&*store_guard, wasm_bytes)
            .map_err(|e| CompileError::compiler_error(&format!("Failed to load module '{}': {}", module_name, e)))?;
        
        let mut modules = self.loaded_modules.lock().unwrap();
        modules.insert(module_name.to_string(), module);
        
        Ok(())
    }

    /// Load a Beanstalk module from file
    pub fn load_module_from_file(&self, module_name: &str, wasm_path: &Path) -> Result<(), CompileError> {
        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| CompileError::file_error(wasm_path, &format!("Failed to read WASM file: {}", e)))?;
        
        self.load_module(module_name, &wasm_bytes)
    }

    /// Execute a function from a loaded module
    pub fn call_function(&self, module_name: &str, function_name: &str, args: &[Value]) -> Result<Vec<Value>, CompileError> {
        let mut store_guard = self.store.lock().unwrap();
        let modules = self.loaded_modules.lock().unwrap();
        
        let module = modules.get(module_name)
            .ok_or_else(|| CompileError::compiler_error(&format!("Module '{}' not loaded", module_name)))?;

        // Create import object (simplified for embedding)
        let import_object = wasmer::imports! {};
        
        let instance = Instance::new(&mut *store_guard, module, &import_object)
            .map_err(|e| CompileError::compiler_error(&format!("Failed to instantiate module '{}': {}", module_name, e)))?;

        let function = instance.exports.get_function(function_name)
            .map_err(|e| CompileError::compiler_error(&format!("Function '{}' not found in module '{}': {}", function_name, module_name, e)))?;

        let result = function.call(&mut *store_guard, args)
            .map_err(|e| CompileError::compiler_error(&format!("Error calling function '{}': {}", function_name, e)))?;

        Ok(result.to_vec())
    }

    /// Reload a module (for hot reloading)
    pub fn reload_module(&self, module_name: &str, wasm_bytes: &[u8]) -> Result<(), CompileError> {
        if !self.hot_reload_enabled {
            return Err(CompileError::compiler_error("Hot reloading is not enabled"));
        }

        // Remove old module
        {
            let mut modules = self.loaded_modules.lock().unwrap();
            modules.remove(module_name);
        }

        // Load new module
        self.load_module(module_name, wasm_bytes)
    }

    /// Get list of loaded modules
    pub fn list_modules(&self) -> Vec<String> {
        let modules = self.loaded_modules.lock().unwrap();
        modules.keys().cloned().collect()
    }

    /// Check if a module is loaded
    pub fn is_module_loaded(&self, module_name: &str) -> bool {
        let modules = self.loaded_modules.lock().unwrap();
        modules.contains_key(module_name)
    }
}

/// Builder for creating embedded runtime configurations
pub struct EmbeddedRuntimeBuilder {
    config: RuntimeConfig,
}

impl EmbeddedRuntimeBuilder {
    pub fn new() -> Self {
        Self {
            config: RuntimeConfig::default(),
        }
    }

    pub fn with_hot_reload(mut self, enabled: bool) -> Self {
        self.config.hot_reload = enabled;
        self
    }

    pub fn with_io_backend(mut self, backend: crate::runtime::IoBackend) -> Self {
        self.config.io_backend = backend;
        self
    }

    pub fn build(self) -> Result<EmbeddedRuntime, CompileError> {
        EmbeddedRuntime::new(&self.config)
    }
}

impl Default for EmbeddedRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}