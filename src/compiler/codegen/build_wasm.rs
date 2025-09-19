use crate::compiler::codegen::wasm_encoding::{WasmModule, LifetimeMemoryStatistics};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::build_mir::MIR;
use crate::compiler::mir::mir_nodes::{ExportKind, MirFunction, Terminator};
use crate::compiler::mir::place::WasmType;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_compiler_error;
use std::collections::HashMap;
use std::time::Instant;

/// WASM validation context for linking validation errors back to MIR
#[derive(Debug)]
struct WasmValidationContext {
    /// Map from WASM function index to MIR function
    function_index_to_mir: HashMap<u32, String>,
    /// Map from WASM type index to MIR type information
    type_index_to_mir: HashMap<u32, String>,
    /// Map from WASM global index to MIR global information
    global_index_to_mir: HashMap<u32, String>,
    /// Source locations for MIR functions
    function_locations: HashMap<String, TextLocation>,
}

impl WasmValidationContext {
    fn new() -> Self {
        Self {
            function_index_to_mir: HashMap::new(),
            type_index_to_mir: HashMap::new(),
            global_index_to_mir: HashMap::new(),
            function_locations: HashMap::new(),
        }
    }

    fn add_function_mapping(&mut self, wasm_index: u32, mir_function: &MirFunction) {
        self.function_index_to_mir
            .insert(wasm_index, mir_function.name.clone());
        // Note: MirFunction doesn't have a direct source_location field
        // We'll use a default location for now
        let default_location = TextLocation::default();
        self.function_locations
            .insert(mir_function.name.clone(), default_location);
    }

    fn add_type_mapping(&mut self, wasm_index: u32, type_name: String) {
        self.type_index_to_mir.insert(wasm_index, type_name);
    }

    fn add_global_mapping(&mut self, wasm_index: u32, global_name: String) {
        self.global_index_to_mir.insert(wasm_index, global_name);
    }

    fn get_function_context(&self, wasm_index: u32) -> Option<(&String, Option<TextLocation>)> {
        self.function_index_to_mir.get(&wasm_index).map(|name| {
            let location = self.function_locations.get(name).cloned();
            (name, location)
        })
    }
}

/// Comprehensive WASM validation with MIR context for detailed error reporting
fn validate_wasm_module_with_mir_context(
    wasm_bytes: &[u8],
    mir: &MIR,
    context: &WasmValidationContext,
) -> Result<(), CompileError> {
    // Perform basic WASM validation first using the simple validate function
    match wasmparser::validate(wasm_bytes) {
        Ok(_) => {
            // Basic validation passed, now perform additional checks
            validate_mir_wasm_consistency(wasm_bytes, mir, context)?;
            validate_wasm_structure_integrity(wasm_bytes, mir, context)?;
            Ok(())
        }
        Err(e) => {
            // Convert wasmparser error to detailed error with MIR context
            convert_wasm_validation_error(e, mir, context)
        }
    }
}

/// Convert wasmparser validation errors to detailed errors with MIR context
fn convert_wasm_validation_error(
    error: wasmparser::BinaryReaderError,
    _mir: &MIR,
    context: &WasmValidationContext,
) -> Result<(), CompileError> {
    let error_message = format!("{}", error);

    // Try to extract function context from error message
    if let Some(function_info) = extract_function_context_from_error(&error_message, context) {
        let (function_name, location) = function_info;
        let location = location.unwrap_or_default();

        return_compiler_error!(
            "WASM validation failed in function '{}' (MIR location {}:{}): {}. \
            This indicates a bug in the WASM backend - the generated WASM bytecode is invalid. \
            Please report this issue with the source code that triggered it.",
            function_name,
            location.start_pos.line_number,
            location.start_pos.char_column,
            error_message
        );
    }

    // Try to extract type context from error message
    if error_message.contains("type") || error_message.contains("signature") {
        return_compiler_error!(
            "WASM type validation failed: {}. \
            This indicates a mismatch between MIR types and generated WASM types. \
            The WASM backend may have incorrectly mapped MIR types to WASM value types. \
            Please report this issue.",
            error_message
        );
    }

    // Try to extract control flow context
    if error_message.contains("control")
        || error_message.contains("branch")
        || error_message.contains("block")
    {
        return_compiler_error!(
            "WASM control flow validation failed: {}. \
            This indicates that the generated WASM control flow structures are invalid. \
            The MIR terminator lowering may have produced incorrect WASM branch instructions. \
            Please report this issue.",
            error_message
        );
    }

    // Try to extract memory context
    if error_message.contains("memory")
        || error_message.contains("load")
        || error_message.contains("store")
    {
        return_compiler_error!(
            "WASM memory validation failed: {}. \
            This indicates invalid memory access patterns in the generated WASM. \
            The MIR place lowering may have produced incorrect memory instructions. \
            Please report this issue.",
            error_message
        );
    }

    // Generic validation error
    return_compiler_error!(
        "WASM module validation failed: {}. \
        This indicates a bug in the WASM backend code generation. \
        Please report this issue with the source code that triggered it.",
        error_message
    );
}

/// Extract function context from wasmparser error messages
fn extract_function_context_from_error<'a>(
    error_message: &str,
    context: &'a WasmValidationContext,
) -> Option<(&'a String, Option<TextLocation>)> {
    // Simple pattern matching without regex for function indices
    if let Some(start) = error_message.find("function ") {
        let after_function = &error_message[start + 9..]; // "function ".len() = 9
        if let Some(end) = after_function.find(|c: char| !c.is_ascii_digit()) {
            if let Ok(function_index) = after_function[..end].parse::<u32>() {
                return context.get_function_context(function_index);
            }
        }
    }

    // Try alternative pattern
    if let Some(start) = error_message.find("at function index ") {
        let after_index = &error_message[start + 18..]; // "at function index ".len() = 18
        if let Some(end) = after_index.find(|c: char| !c.is_ascii_digit()) {
            if let Ok(function_index) = after_index[..end].parse::<u32>() {
                return context.get_function_context(function_index);
            }
        }
    }

    None
}

/// Validate MIR-WASM consistency with basic checks
fn validate_mir_wasm_consistency(
    _wasm_bytes: &[u8],
    mir: &MIR,
    context: &WasmValidationContext,
) -> Result<(), CompileError> {
    // Empty MIR is valid - skip function validation
    if mir.functions.is_empty() {
        return Ok(());
    }
    
    // Validate that we have the expected number of functions
    if context.function_index_to_mir.len() != mir.functions.len() {
        return_compiler_error!(
            "Function count mismatch: MIR has {} functions but WASM context tracks {} functions. \
            This indicates a bug in function compilation or index tracking.",
            mir.functions.len(),
            context.function_index_to_mir.len()
        );
    }

    // Validate that all MIR functions are represented in the context
    for (index, mir_function) in mir.functions.iter().enumerate() {
        if !context.function_index_to_mir.contains_key(&(index as u32)) {
            return_compiler_error!(
                "MIR function '{}' at index {} is not tracked in WASM validation context. \
                This indicates a bug in function compilation tracking.",
                mir_function.name,
                index
            );
        }
    }

    // Validate function signatures consistency
    for mir_function in &mir.functions {
        validate_mir_function_signature(mir_function)?;
    }

    Ok(())
}

/// Validate MIR function signature for WASM compatibility
fn validate_mir_function_signature(mir_function: &MirFunction) -> Result<(), CompileError> {
    // Validate parameter types are WASM-compatible
    for (param_index, param_place) in mir_function.parameters.iter().enumerate() {
        let wasm_type = param_place.wasm_type();
        if !is_valid_wasm_type(&wasm_type) {
            return_compiler_error!(
                "Function '{}' parameter {} has invalid WASM type {:?}. \
                Only i32, i64, f32, f64, externref, and funcref are supported in WASM.",
                mir_function.name,
                param_index,
                wasm_type
            );
        }
    }

    // Validate return types are WASM-compatible
    for (return_index, return_type) in mir_function.return_types.iter().enumerate() {
        if !is_valid_wasm_type(return_type) {
            return_compiler_error!(
                "Function '{}' return type {} has invalid WASM type {:?}. \
                Only i32, i64, f32, f64, externref, and funcref are supported in WASM.",
                mir_function.name,
                return_index,
                return_type
            );
        }
    }

    // Validate function signature consistency
    if mir_function.signature.param_types.len() != mir_function.parameters.len() {
        return_compiler_error!(
            "Function '{}' has {} parameters but signature specifies {} parameter types. \
            This indicates inconsistent function signature generation.",
            mir_function.name,
            mir_function.parameters.len(),
            mir_function.signature.param_types.len()
        );
    }

    if mir_function.signature.result_types.len() != mir_function.return_types.len() {
        return_compiler_error!(
            "Function '{}' has {} return types but signature specifies {} result types. \
            This indicates inconsistent function signature generation.",
            mir_function.name,
            mir_function.return_types.len(),
            mir_function.signature.result_types.len()
        );
    }

    Ok(())
}

/// Check if a WASM type is valid for WASM modules
fn is_valid_wasm_type(wasm_type: &WasmType) -> bool {
    matches!(
        wasm_type,
        WasmType::I32
            | WasmType::I64
            | WasmType::F32
            | WasmType::F64
            | WasmType::ExternRef
            | WasmType::FuncRef
    )
}

/// Validate WASM structure integrity with basic checks
fn validate_wasm_structure_integrity(
    _wasm_bytes: &[u8],
    mir: &MIR,
    _context: &WasmValidationContext,
) -> Result<(), CompileError> {
    // Validate MIR control flow structure for WASM compatibility
    for mir_function in &mir.functions {
        validate_mir_control_flow_structure(mir_function)?;
    }

    // Validate memory requirements
    validate_mir_memory_requirements(mir)?;

    // Validate interface requirements if present
    if !mir.type_info.interface_info.interfaces.is_empty() {
        validate_mir_interface_requirements(mir)?;
    }

    Ok(())
}

/// Validate MIR control flow structure for WASM compatibility
fn validate_mir_control_flow_structure(mir_function: &MirFunction) -> Result<(), CompileError> {
    // Validate terminator targets are within bounds
    for (block_index, block) in mir_function.blocks.iter().enumerate() {
        validate_terminator_targets(mir_function, block_index, &block.terminator)?;
    }

    // Validate that the function has at least one block
    if mir_function.blocks.is_empty() {
        return_compiler_error!(
            "Function '{}' has no blocks. \
            All MIR functions must have at least one block for WASM code generation.",
            mir_function.name
        );
    }

    Ok(())
}

/// Validate terminator targets are within function bounds
fn validate_terminator_targets(
    mir_function: &MirFunction,
    block_index: usize,
    terminator: &Terminator,
) -> Result<(), CompileError> {
    match terminator {
        Terminator::Goto { target, .. } => {
            if *target as usize >= mir_function.blocks.len() {
                return_compiler_error!(
                    "Function '{}' block {} goto targets block {} but only {} blocks exist. \
                    This indicates invalid control flow in MIR construction.",
                    mir_function.name,
                    block_index,
                    target,
                    mir_function.blocks.len()
                );
            }
        }
        Terminator::If {
            then_block,
            else_block,
            ..
        } => {
            if *then_block as usize >= mir_function.blocks.len() {
                return_compiler_error!(
                    "Function '{}' block {} if-then targets block {} but only {} blocks exist. \
                    This indicates invalid control flow in MIR construction.",
                    mir_function.name,
                    block_index,
                    then_block,
                    mir_function.blocks.len()
                );
            }
            if *else_block as usize >= mir_function.blocks.len() {
                return_compiler_error!(
                    "Function '{}' block {} if-else targets block {} but only {} blocks exist. \
                    This indicates invalid control flow in MIR construction.",
                    mir_function.name,
                    block_index,
                    else_block,
                    mir_function.blocks.len()
                );
            }
        }
        Terminator::Switch {
            targets, default, ..
        } => {
            for (case_index, target) in targets.iter().enumerate() {
                if *target as usize >= mir_function.blocks.len() {
                    return_compiler_error!(
                        "Function '{}' block {} switch case {} targets block {} but only {} blocks exist. \
                        This indicates invalid control flow in MIR construction.",
                        mir_function.name,
                        block_index,
                        case_index,
                        target,
                        mir_function.blocks.len()
                    );
                }
            }
            if *default as usize >= mir_function.blocks.len() {
                return_compiler_error!(
                    "Function '{}' block {} switch default targets block {} but only {} blocks exist. \
                    This indicates invalid control flow in MIR construction.",
                    mir_function.name,
                    block_index,
                    default,
                    mir_function.blocks.len()
                );
            }
        }
        _ => {} // Other terminators don't have block targets
    }

    Ok(())
}

/// Validate MIR memory requirements for WASM compatibility
fn validate_mir_memory_requirements(mir: &MIR) -> Result<(), CompileError> {
    // Validate memory configuration
    if mir.type_info.memory_info.initial_pages == 0
        && mir.type_info.memory_info.static_data_size > 0
    {
        return_compiler_error!(
            "MIR requires {} bytes of static data but specifies 0 initial memory pages. \
            This indicates inconsistent memory configuration in MIR.",
            mir.type_info.memory_info.static_data_size
        );
    }

    // Validate memory limits
    if let Some(max_pages) = mir.type_info.memory_info.max_pages {
        if max_pages < mir.type_info.memory_info.initial_pages {
            return_compiler_error!(
                "MIR memory max pages ({}) is less than initial pages ({}). \
                This indicates invalid memory configuration.",
                max_pages,
                mir.type_info.memory_info.initial_pages
            );
        }
    }

    // Validate static data size is reasonable
    let max_static_size = mir.type_info.memory_info.initial_pages * 65536; // 64KB per page
    if mir.type_info.memory_info.static_data_size > max_static_size {
        return_compiler_error!(
            "MIR static data size ({} bytes) exceeds initial memory size ({} bytes). \
            This indicates insufficient memory allocation for static data.",
            mir.type_info.memory_info.static_data_size,
            max_static_size
        );
    }

    Ok(())
}

/// Validate MIR interface requirements for WASM compatibility
fn validate_mir_interface_requirements(mir: &MIR) -> Result<(), CompileError> {
    // Validate interface definitions
    for (interface_id, interface_def) in &mir.type_info.interface_info.interfaces {
        if interface_def.methods.is_empty() {
            return_compiler_error!(
                "Interface {} has no methods. \
                Empty interfaces are not useful for WASM dynamic dispatch.",
                interface_id
            );
        }

        // Validate method signatures
        for method in &interface_def.methods {
            for param_type in &method.param_types {
                if !is_valid_wasm_type(param_type) {
                    return_compiler_error!(
                        "Interface {} method {} has invalid WASM parameter type {:?}. \
                        Only i32, i64, f32, f64, externref, and funcref are supported.",
                        interface_id,
                        method.id,
                        param_type
                    );
                }
            }

            for return_type in &method.return_types {
                if !is_valid_wasm_type(return_type) {
                    return_compiler_error!(
                        "Interface {} method {} has invalid WASM return type {:?}. \
                        Only i32, i64, f32, f64, externref, and funcref are supported.",
                        interface_id,
                        method.id,
                        return_type
                    );
                }
            }
        }
    }

    // Validate function table configuration
    if !mir.type_info.interface_info.function_table.is_empty() {
        for (table_index, function_index) in mir
            .type_info
            .interface_info
            .function_table
            .iter()
            .enumerate()
        {
            if *function_index as usize >= mir.functions.len() {
                return_compiler_error!(
                    "Function table entry {} references function index {} but only {} functions exist. \
                    This indicates invalid function table construction.",
                    table_index,
                    function_index,
                    mir.functions.len()
                );
            }
        }
    }

    Ok(())
}

/// Complete MIR-to-WASM compilation entry point with performance logging and validation
/// 
/// This function replaces the stub implementation with a comprehensive MIR processing
/// system that generates efficient, validated WASM bytecode from the refined MIR structure.
/// 
/// # Performance Characteristics
/// - Direct MIR → WASM lowering without intermediate representations
/// - Efficient place resolution using pre-computed mappings
/// - Minimal overhead from WASM-first MIR design
/// 
/// # Error Handling
/// - Comprehensive MIR validation before WASM generation
/// - Clear error messages linking WASM validation errors back to MIR statements
/// - Proper integration with existing compiler pipeline error reporting
pub fn new_wasm_module(mir: MIR) -> Result<Vec<u8>, CompileError> {
    let compilation_start = Instant::now();
    
    // Phase 1: MIR Validation and Preprocessing
    let validation_start = Instant::now();
    validate_mir_for_wasm_compilation(&mir)?;
    let validation_time = validation_start.elapsed();
    
    // Phase 2: WASM Module Initialization
    let init_start = Instant::now();
    let mut module = WasmModule::from_mir(&mir)?;
    let init_time = init_start.elapsed();
    
    // Build validation context for tracking MIR to WASM mappings
    let mut validation_context = WasmValidationContext::new();
    
    // Phase 3: Function Compilation
    let function_compilation_start = Instant::now();
    let mut compiled_functions = 0;
    let mut total_mir_statements = 0;
    
    // Handle empty MIR case - create minimal WASM module
    if mir.functions.is_empty() {
        // For empty MIR, we still need to create a valid WASM module
        // This is useful for testing and incremental compilation scenarios
        eprintln!("Creating minimal WASM module (no functions in MIR)");
    } else {
        for mir_function in &mir.functions {
            let function_start = Instant::now();
            
            // Count MIR statements for metrics
            let statement_count: usize = mir_function.blocks.iter()
                .map(|block| block.statements.len() + 1) // +1 for terminator
                .sum();
            total_mir_statements += statement_count;
            
            // Compile function (lifetime optimization will be integrated when borrow checking results are available)
            let function_index = module.compile_mir_function(mir_function)?;
            
            compiled_functions += 1;
            
            // Track function mapping for validation
            validation_context.add_function_mapping(function_index, mir_function);
            
            // Export the function if it's marked for export
            if let Some(export) = mir.exports.get(&mir_function.name) {
                if export.kind == ExportKind::Function {
                    module.add_function_export(&export.name, function_index);
                }
            }
            
            let function_time = function_start.elapsed();
            if function_time.as_millis() > 100 {
                eprintln!("  Function '{}' compilation took {}ms ({} statements)", 
                         mir_function.name, function_time.as_millis(), statement_count);
            }
        }
    }
    
    let function_compilation_time = function_compilation_start.elapsed();
    
    // Phase 4: Export Generation
    let export_start = Instant::now();
    let mut exported_items = 0;
    
    for (export_name, export) in &mir.exports {
        match export.kind {
            ExportKind::Global => {
                module.add_global_export(export_name, export.index);
                exported_items += 1;
            }
            ExportKind::Memory => {
                module.add_memory_export(export_name, export.index);
                exported_items += 1;
            }
            ExportKind::Table => {
                // Table exports handled by interface support
                exported_items += 1;
            }
            _ => {} // Function exports handled above
        }
    }
    
    let export_time = export_start.elapsed();
    
    // Phase 5: Interface Support (if needed)
    let interface_start = Instant::now();
    let mut interface_methods = 0;
    
    if !mir.type_info.interface_info.interfaces.is_empty() {
        for interface_def in mir.type_info.interface_info.interfaces.values() {
            interface_methods += interface_def.methods.len();
        }
        
        // Interface support is already initialized in from_mir, just log metrics
        eprintln!("  Generated interface support for {} methods", interface_methods);
    }
    
    let interface_time = interface_start.elapsed();
    
    // Phase 6: Memory Management Statistics
    let stats_start = Instant::now();
    let stats = module.get_lifetime_memory_statistics();
    let stats_time = stats_start.elapsed();
    
    // Phase 7: Validation Context Setup
    let context_start = Instant::now();
    
    // Add type and global mappings to validation context
    for (index, _function_type) in mir.type_info.function_types.iter().enumerate() {
        validation_context.add_type_mapping(index as u32, format!("function_type_{}", index));
    }
    
    for (index, (global_name, _)) in mir.globals.iter().enumerate() {
        validation_context.add_global_mapping(index as u32, format!("global_{}", global_name));
    }
    
    let context_time = context_start.elapsed();
    
    // Phase 8: WASM Module Finalization
    let finalization_start = Instant::now();
    let compiled_wasm = module.finish();
    let finalization_time = finalization_start.elapsed();
    
    // Phase 9: WASM Validation
    let wasm_validation_start = Instant::now();
    validate_wasm_module_with_mir_context(&compiled_wasm, &mir, &validation_context)?;
    let wasm_validation_time = wasm_validation_start.elapsed();
    
    let total_compilation_time = compilation_start.elapsed();
    
    // Performance Logging and Compilation Metrics
    log_compilation_metrics(CompilationMetrics {
        total_time: total_compilation_time,
        validation_time,
        initialization_time: init_time,
        function_compilation_time,
        export_time,
        interface_time,
        stats_time,
        context_time,
        finalization_time,
        wasm_validation_time,
        compiled_functions,
        total_mir_statements,
        exported_items,
        interface_methods,
        wasm_module_size: compiled_wasm.len(),
        memory_stats: stats,
    });
    
    Ok(compiled_wasm)
}

/// Compilation metrics for performance monitoring and optimization
#[derive(Debug)]
struct CompilationMetrics {
    total_time: std::time::Duration,
    validation_time: std::time::Duration,
    initialization_time: std::time::Duration,
    function_compilation_time: std::time::Duration,
    export_time: std::time::Duration,
    interface_time: std::time::Duration,
    stats_time: std::time::Duration,
    context_time: std::time::Duration,
    finalization_time: std::time::Duration,
    wasm_validation_time: std::time::Duration,
    compiled_functions: usize,
    total_mir_statements: usize,
    exported_items: usize,
    interface_methods: usize,
    wasm_module_size: usize,
    memory_stats: LifetimeMemoryStatistics,
}

/// Log compilation metrics with performance analysis
fn log_compilation_metrics(metrics: CompilationMetrics) {
    let total_ms = metrics.total_time.as_millis();
    
    // Only log detailed metrics for non-trivial compilations
    if total_ms > 10 || metrics.compiled_functions > 5 {
        eprintln!("\n=== WASM Compilation Metrics ===");
        eprintln!("Total compilation time: {}ms", total_ms);
        eprintln!("  MIR validation: {}ms", metrics.validation_time.as_millis());
        eprintln!("  Module initialization: {}ms", metrics.initialization_time.as_millis());
        eprintln!("  Function compilation: {}ms", metrics.function_compilation_time.as_millis());
        eprintln!("  Export generation: {}ms", metrics.export_time.as_millis());
        eprintln!("  Interface support: {}ms", metrics.interface_time.as_millis());
        eprintln!("  Statistics collection: {}ms", metrics.stats_time.as_millis());
        eprintln!("  Validation context: {}ms", metrics.context_time.as_millis());
        eprintln!("  Module finalization: {}ms", metrics.finalization_time.as_millis());
        eprintln!("  WASM validation: {}ms", metrics.wasm_validation_time.as_millis());
        
        eprintln!("\n=== Code Generation Statistics ===");
        eprintln!("Functions compiled: {}", metrics.compiled_functions);
        eprintln!("MIR statements processed: {}", metrics.total_mir_statements);
        eprintln!("Items exported: {}", metrics.exported_items);
        eprintln!("Interface methods: {}", metrics.interface_methods);
        eprintln!("WASM module size: {} bytes", metrics.wasm_module_size);
        
        // Performance analysis
        if metrics.compiled_functions > 0 {
            let avg_statements_per_function = metrics.total_mir_statements / metrics.compiled_functions;
            let statements_per_ms = if metrics.function_compilation_time.as_millis() > 0 {
                metrics.total_mir_statements as u128 / metrics.function_compilation_time.as_millis()
            } else {
                0
            };
            
            eprintln!("Average statements per function: {}", avg_statements_per_function);
            eprintln!("Statements compiled per ms: {}", statements_per_ms);
        }
    }
    
    // Always log memory management statistics if optimizations were applied
    if metrics.memory_stats.single_ownership_optimizations > 0 
        || metrics.memory_stats.arc_operations_eliminated > 0 {
        eprintln!("\n=== Lifetime Memory Management Statistics ===");
        eprintln!("Single ownership optimizations: {}", metrics.memory_stats.single_ownership_optimizations);
        eprintln!("ARC operations eliminated: {}", metrics.memory_stats.arc_operations_eliminated);
        eprintln!("Move optimizations applied: {}", metrics.memory_stats.move_optimizations_applied);
        eprintln!("Drop operations optimized: {}", metrics.memory_stats.drop_operations_optimized);
        eprintln!("Memory allocation reduction: {} bytes", metrics.memory_stats.memory_allocation_reduction);
        eprintln!("Instruction count reduction: {}", metrics.memory_stats.instruction_count_reduction);
    }
    
    // Performance warnings for slow compilation
    if total_ms > 1000 {
        eprintln!("\n⚠️  Compilation took over 1 second. Consider optimizing MIR structure or function complexity.");
    }
    
    if metrics.wasm_module_size > 1024 * 1024 {
        eprintln!("⚠️  Generated WASM module is over 1MB. Consider code splitting or optimization.");
    }
}

/// Validate MIR structure before WASM compilation
fn validate_mir_for_wasm_compilation(mir: &MIR) -> Result<(), CompileError> {
    // Empty MIR is allowed - it creates a minimal WASM module
    // This is useful for testing and incremental compilation
    
    // Validate function names are unique (if any functions exist)
    if !mir.functions.is_empty() {
        let mut function_names = std::collections::HashSet::new();
        for function in &mir.functions {
            if !function_names.insert(&function.name) {
                return_compiler_error!(
                    "Duplicate function name '{}' in MIR. Function names must be unique for WASM generation.",
                    function.name
                );
            }
        }
    }
    
    // Validate memory configuration is reasonable
    let memory_info = &mir.type_info.memory_info;
    if memory_info.initial_pages > 65536 {
        return_compiler_error!(
            "MIR specifies {} initial pages, but WASM maximum is 65536 pages (4GB).",
            memory_info.initial_pages
        );
    }
    
    if let Some(max_pages) = memory_info.max_pages {
        if max_pages > 65536 {
            return_compiler_error!(
                "MIR specifies {} max pages, but WASM maximum is 65536 pages (4GB).",
                max_pages
            );
        }
        
        // Check that max pages is not less than initial pages
        if max_pages < memory_info.initial_pages {
            return_compiler_error!(
                "MIR memory max pages ({}) is less than initial pages ({}). \
                Maximum memory pages must be greater than or equal to initial pages.",
                max_pages,
                memory_info.initial_pages
            );
        }
    }
    
    // Validate interface consistency
    if !mir.type_info.interface_info.interfaces.is_empty() {
        for (interface_id, interface_def) in &mir.type_info.interface_info.interfaces {
            if interface_def.methods.is_empty() {
                return_compiler_error!(
                    "Interface {} has no methods. Empty interfaces cannot be used for dynamic dispatch.",
                    interface_id
                );
            }
        }
    }
    
    Ok(())
}
