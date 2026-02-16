//! WASM Codegen Encoder
//!
//! Encodes LIR into WASM bytes using the wasm_encoder library (v0.243.0).
//! This is the main entry point for the LIR to WASM codegen system.
//!
//! ## Architecture
//!
//! The encoder orchestrates all codegen components:
//! - LirAnalyzer: Extracts type information and builds mapping tables
//! - WasmModuleBuilder: Constructs WASM modules with proper section ordering
//! - InstructionLowerer: Converts LIR instructions to WASM bytecode
//! - ControlFlowManager: Handles structured control flow generation
//! - MemoryManager: Sets up WASM linear memory and bump allocator
//! - OwnershipManager: Handles tagged pointer ownership system
//! - HostFunctionManager: Manages host function imports and exports
//!
//! ## Encoding Pipeline
//!
//! 1. Analyze LIR module (extract types, signatures, layouts)
//! 2. Set up host function imports (must come before functions)
//! 3. Set up memory (linear memory, globals, alloc/free functions)
//! 4. Process each function:
//!    a. Create function type and add to type section
//!    b. Build local mapping
//!    c. Lower LIR instructions to WASM
//!    d. Add function to module
//! 5. Add exports (main function, memory)
//! 6. Validate and finalize module

use crate::backends::function_registry::HostRegistry;
use crate::backends::lir::nodes::{LirFunction, LirInst, LirModule};
use crate::backends::wasm::{
    analyzer::LirAnalyzer,
    control_flow::ControlFlowManager,
    error::WasmGenerationError,
    host_functions::{FunctionExport, HostFunctionManager, MemoryExport},
    instruction_lowerer::InstructionLowerer,
    memory_manager::{MemoryConfig, MemoryManager},
    module_builder::WasmModuleBuilder,
    ownership_manager::OwnershipManager,
    validator::{WasmValidator, validate_wasm_module_comprehensive},
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::string_interning::StringTable;
use std::collections::HashMap;
use wasm_encoder::{ExportKind, Function, Instruction, ValType};

/// Encode a LIR module into a vector of WASM bytes.
///
/// This is the main entry point for WASM codegen. It orchestrates all the
/// components to transform LIR into valid WASM bytecode.
///
/// This version does not include host function support. For host function
/// integration, use `encode_wasm_with_host_functions`.
pub fn encode_wasm(lir: &LirModule) -> Result<Vec<u8>, CompilerError> {
    // Phase 1: Analyze the LIR module
    let analyzer = LirAnalyzer::analyze_module(lir)?;

    // Phase 2: Create WASM module builder
    let mut module_builder = WasmModuleBuilder::new();

    // Phase 3: Set up memory using MemoryManager
    let mut memory_manager = MemoryManager::with_config(MemoryConfig::default());
    let memory_indices = memory_manager.setup_memory(&mut module_builder)?;

    // Phase 4: Create ownership manager with memory indices
    let ownership_manager = OwnershipManager::new(
        memory_indices.alloc_func_index,
        memory_indices.free_func_index,
    );

    // Phase 5: Build function index map (accounting for internal functions)
    // Internal functions (__bst_alloc, __bst_free) are already added
    let internal_func_count = module_builder.total_function_count();
    let mut function_indices: HashMap<String, u32> = HashMap::new();
    for (i, lir_func) in lir.functions.iter().enumerate() {
        function_indices.insert(lir_func.name.clone(), internal_func_count + i as u32);
    }

    // Phase 6: Process functions
    let mut main_function_index: Option<u32> = None;

    for lir_function in &lir.functions {
        let function_index = encode_function(
            lir_function,
            &analyzer,
            &function_indices,
            &ownership_manager,
            &mut module_builder,
        )?;

        // Track main function for export
        if lir_function.is_main {
            main_function_index = Some(function_index);
        }
    }

    // Phase 7: Add exports
    if let Some(main_idx) = main_function_index {
        module_builder.add_export("main", ExportKind::Func, main_idx);
    }

    // Phase 8: Validate and finalize the module
    module_builder
        .validate()
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    let wasm_bytes = module_builder.finish()?;

    // Phase 9: Validate the generated WASM using comprehensive wasmparser validation
    validate_wasm_module_comprehensive(&wasm_bytes, "Generated WASM module")
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    Ok(wasm_bytes)
}

/// Encode a LIR module into WASM bytes with host function support.
///
/// This version accepts a host function registry to properly generate
/// WASM imports for host functions like host_io_functions().
///
/// ## Pipeline
///
/// 1. Analyze LIR module
/// 2. Set up host function imports (must come first for proper indexing)
/// 3. Set up memory (linear memory, globals, alloc/free)
/// 4. Process each function with full instruction lowering
/// 5. Add exports (main, memory)
/// 6. Validate and finalize
#[allow(dead_code)]
pub fn encode_wasm_with_host_functions(
    lir: &LirModule,
    host_registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<Vec<u8>, CompilerError> {
    // Phase 1: Analyze the LIR module
    let analyzer = LirAnalyzer::analyze_module(lir)?;

    // Phase 2: Create WASM module builder
    let mut module_builder = WasmModuleBuilder::new();

    // Phase 3: Set up host function imports (must come before functions)
    let mut host_manager = HostFunctionManager::new();
    host_manager.register_from_registry(host_registry, string_table)?;

    // Apply imports to the module builder
    let host_function_indices = host_manager.apply_imports(&mut module_builder)?;

    // Phase 4: Set up memory using MemoryManager
    let mut memory_manager = MemoryManager::with_config(MemoryConfig::default());
    let memory_indices = memory_manager.setup_memory(&mut module_builder)?;

    // Phase 5: Create ownership manager with memory indices
    let ownership_manager = OwnershipManager::new(
        memory_indices.alloc_func_index,
        memory_indices.free_func_index,
    );

    // Phase 6: Build complete function index map
    // Order: host imports -> internal functions -> user functions
    let mut function_indices: HashMap<String, u32> = host_function_indices;
    let user_func_start = module_builder.total_function_count();
    for (i, lir_func) in lir.functions.iter().enumerate() {
        function_indices.insert(lir_func.name.clone(), user_func_start + i as u32);
    }

    // Phase 7: Process functions
    let mut main_function_index: Option<u32> = None;

    for lir_function in &lir.functions {
        let function_index = encode_function(
            lir_function,
            &analyzer,
            &function_indices,
            &ownership_manager,
            &mut module_builder,
        )?;

        // Track main function for export
        if lir_function.is_main {
            main_function_index = Some(function_index);
        }
    }

    // Phase 8: Add exports
    // Export main function if present
    if let Some(main_idx) = main_function_index {
        host_manager.register_export(FunctionExport::main(main_idx));
    }

    // Export memory for host access (standard WASM pattern)
    host_manager.register_memory_export(MemoryExport::standard());

    // Validate exports before applying
    host_manager
        .validate_exports(&module_builder)
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    // Apply exports to the module builder
    host_manager.apply_exports(&mut module_builder);

    // Phase 9: Validate and finalize the module
    module_builder
        .validate()
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    // Get counts before consuming the module builder
    let total_function_count = module_builder.total_function_count();
    let total_type_count = module_builder.type_count();
    let total_memory_count = module_builder.total_memory_count();
    let total_global_count = module_builder.total_global_count();

    let wasm_bytes = module_builder.finish()?;

    // Phase 10: Validate the generated WASM using comprehensive wasmparser validation
    let mut validator = WasmValidator::new();

    // Set function names for better error reporting
    let mut function_names = HashMap::new();
    for (i, lir_function) in lir.functions.iter().enumerate() {
        let function_index = host_manager.import_count() + i as u32;
        function_names.insert(function_index, lir_function.name.clone());
    }
    validator.set_function_names(function_names);

    // Perform comprehensive validation
    validator
        .validate_module(&wasm_bytes)
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    // Additional validation checks
    validator
        .validate_function_calls(&wasm_bytes, total_function_count)
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    // Comprehensive index consistency validation
    validator
        .validate_comprehensive_index_consistency(
            &wasm_bytes,
            total_function_count,
            total_type_count,
            total_memory_count,
            total_global_count,
        )
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;

    Ok(wasm_bytes)
}

/// Encode a single LIR function into WASM.
///
/// This function handles:
/// - Function type creation
/// - Local variable mapping
/// - Instruction lowering with control flow and ownership support
/// - Return handling
fn encode_function(
    lir_function: &LirFunction,
    analyzer: &LirAnalyzer,
    function_indices: &HashMap<String, u32>,
    ownership_manager: &OwnershipManager,
    module_builder: &mut WasmModuleBuilder,
) -> Result<u32, CompilerError> {
    // Get function signature
    let signature = analyzer
        .get_function_signature(&lir_function.name)
        .ok_or_else(|| {
            WasmGenerationError::lir_analysis("Function signature not found", &lir_function.name)
                .to_compiler_error(ErrorLocation::default())
        })?;

    // Create WASM function type
    let params: Vec<ValType> = signature
        .parameters
        .iter()
        .map(|t| t.to_val_type())
        .collect();
    let results: Vec<ValType> = signature.returns.iter().map(|t| t.to_val_type()).collect();
    let type_index = module_builder.add_function_type(params.clone(), results.clone());

    // Get local mapping
    let local_mapping = analyzer
        .get_local_mapping(&lir_function.name)
        .ok_or_else(|| {
            WasmGenerationError::lir_analysis("Local mapping not found", &lir_function.name)
                .to_compiler_error(ErrorLocation::default())
        })?;

    // Create WASM function with locals (clone to preserve local_mapping for instruction lowerer)
    let wasm_locals = local_mapping.local_types.clone();
    let mut function = Function::new(wasm_locals);

    // Create instruction lowerer with local mapping and function indices
    let instruction_lowerer =
        InstructionLowerer::with_function_indices(local_mapping, function_indices.clone());

    // Create control flow manager for this function
    let mut control_flow_manager = ControlFlowManager::new();

    // Lower all LIR instructions to WASM
    if !lir_function.body.is_empty() {
        instruction_lowerer.lower_instructions_full(
            &lir_function.body,
            &mut function,
            &mut control_flow_manager,
            ownership_manager,
        )?;
    }

    // Handle return value for functions with no explicit return
    // If the function has return types but the body doesn't end with a return,
    // we need to ensure proper stack state
    ensure_function_return(&lir_function.body, &results, &mut function)?;

    // Validate all control flow blocks are closed
    control_flow_manager.validate_all_blocks_closed()?;

    // Add End instruction (wasm_encoder handles this automatically for Function)
    function.instruction(&Instruction::End);

    // Add the function to the module
    let function_index = module_builder.add_function(type_index, function);

    Ok(function_index)
}

/// Ensure a function has proper return handling.
///
/// This function checks if the function body ends with a return instruction.
/// If not, and the function has return types, it adds default return values.
///
/// For void functions (no return types), no action is needed.
/// For functions with return types, we check if the last instruction is a Return.
/// If not, we add default values to satisfy the stack requirements.
fn ensure_function_return(
    body: &[LirInst],
    return_types: &[ValType],
    function: &mut Function,
) -> Result<(), CompilerError> {
    // If no return types, nothing to do
    if return_types.is_empty() {
        return Ok(());
    }

    // Check if the body ends with a Return instruction
    let ends_with_return = body
        .last()
        .map_or(false, |inst| matches!(inst, LirInst::Return));

    // If the body doesn't end with return and we have return types,
    // add default values for each return type
    if !ends_with_return {
        for return_type in return_types {
            match return_type {
                ValType::I32 => {
                    function.instruction(&Instruction::I32Const(0));
                }
                ValType::I64 => {
                    function.instruction(&Instruction::I64Const(0));
                }
                ValType::F32 => {
                    function.instruction(&Instruction::F32Const(0.0_f32.into()));
                }
                ValType::F64 => {
                    function.instruction(&Instruction::F64Const(0.0_f64.into()));
                }
                _ => {
                    return Err(WasmGenerationError::instruction_lowering(
                        format!("Unsupported return type: {:?}", return_type),
                        "Cannot generate default value for this type",
                    )
                    .to_compiler_error(ErrorLocation::default()));
                }
            }
        }
    }

    Ok(())
}

/// Get the WASM function index for a host function by name.
///
/// This is useful for generating call instructions to host functions.
#[allow(dead_code)]
pub fn get_host_function_index(
    host_manager: &HostFunctionManager,
    function_name: &str,
) -> Option<u32> {
    host_manager.get_function_index(function_name)
}

/// Validate the generated WASM module using wasmparser.
///
/// This performs basic validation to ensure the module is well-formed.
#[allow(dead_code)]
fn validate_wasm_module(wasm_bytes: &[u8]) -> Result<(), CompilerError> {
    use crate::backends::wasm::error::validate_wasm_bytes;

    validate_wasm_bytes(wasm_bytes, "Generated WASM module")
        .map_err(|e| e.to_compiler_error(ErrorLocation::default()))
}

/// Context for encoding a complete WASM module.
///
/// This struct holds all the components needed for WASM generation
/// and provides a convenient interface for the encoding process.
#[allow(dead_code)]
pub struct WasmEncodingContext {
    /// The LIR analyzer with extracted type information
    pub analyzer: LirAnalyzer,
    /// The WASM module builder
    pub module_builder: WasmModuleBuilder,
    /// The memory manager
    pub memory_manager: MemoryManager,
    /// The ownership manager
    pub ownership_manager: Option<OwnershipManager>,
    /// The host function manager
    pub host_manager: HostFunctionManager,
    /// Function name to index mapping
    pub function_indices: HashMap<String, u32>,
}

impl WasmEncodingContext {
    /// Create a new encoding context from a LIR module.
    #[allow(dead_code)]
    pub fn new(lir: &LirModule) -> Result<Self, CompilerError> {
        let analyzer = LirAnalyzer::analyze_module(lir)?;
        let module_builder = WasmModuleBuilder::new();
        let memory_manager = MemoryManager::with_config(MemoryConfig::default());
        let host_manager = HostFunctionManager::new();

        Ok(WasmEncodingContext {
            analyzer,
            module_builder,
            memory_manager,
            ownership_manager: None,
            host_manager,
            function_indices: HashMap::new(),
        })
    }

    /// Set up memory and create the ownership manager.
    #[allow(dead_code)]
    pub fn setup_memory(&mut self) -> Result<(), CompilerError> {
        let indices = self.memory_manager.setup_memory(&mut self.module_builder)?;
        self.ownership_manager = Some(OwnershipManager::new(
            indices.alloc_func_index,
            indices.free_func_index,
        ));
        Ok(())
    }

    /// Get the ownership manager (must call setup_memory first).
    #[allow(dead_code)]
    pub fn ownership_manager(&self) -> Result<&OwnershipManager, CompilerError> {
        self.ownership_manager.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Ownership manager not initialized",
                "WasmEncodingContext",
                Some("Call setup_memory() first".to_string()),
            )
            .to_compiler_error(ErrorLocation::default())
        })
    }
}
