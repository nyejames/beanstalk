use crate::build;
use crate::build::OutputFile;
use std::path::Path;

/// Test basic WASIX print functionality compilation
#[test]
fn test_wasix_print_basic_compilation() {
    let test_path = Path::new("tests/cases/wasix_print_basic.bst");
    
    // Build the project
    let project = build::build_project_files(test_path, false, &[])
        .expect("Failed to build WASIX test project");
    
    // Extract WASM bytes
    let wasm_bytes = match &project.output_files[0] {
        OutputFile::Wasm(bytes) => bytes,
        _ => panic!("Expected WASM output"),
    };
    
    println!("✓ WASIX print compilation successful");
    println!("  WASM size: {} bytes", wasm_bytes.len());
    
    // Validate WASM module structure
    validate_wasix_imports(wasm_bytes).expect("WASIX import validation failed");
    
    println!("✓ WASIX imports validated successfully");
}

/// Test WASIX print with variables
#[test]
fn test_wasix_print_variables() {
    // Create a test file for variable printing
    let test_content = r#"message = "Hello from WASIX"
print(message)"#;
    
    // Write to a temporary test file
    std::fs::write("tests/cases/wasix_print_variables.bst", test_content)
        .expect("Failed to write test file");
    
    let test_path = Path::new("tests/cases/wasix_print_variables.bst");
    
    // Build the project
    let project = build::build_project_files(test_path, false, &[])
        .expect("Failed to build WASIX variable test project");
    
    // Extract WASM bytes
    let wasm_bytes = match &project.output_files[0] {
        OutputFile::Wasm(bytes) => bytes,
        _ => panic!("Expected WASM output"),
    };
    
    println!("✓ WASIX variable print compilation successful");
    validate_wasix_imports(wasm_bytes).expect("WASIX import validation failed");
}

/// Test multiple WASIX print statements
#[test]
fn test_wasix_multiple_prints() {
    // Create a test file for multiple prints
    let test_content = r#"print("First message")
print("Second message")
print("Third message")"#;
    
    // Write to a temporary test file
    std::fs::write("tests/cases/wasix_print_multiple.bst", test_content)
        .expect("Failed to write test file");
    
    let test_path = Path::new("tests/cases/wasix_print_multiple.bst");
    
    // Build the project
    let project = build::build_project_files(test_path, false, &[])
        .expect("Failed to build WASIX multiple prints test project");
    
    // Extract WASM bytes
    let wasm_bytes = match &project.output_files[0] {
        OutputFile::Wasm(bytes) => bytes,
        _ => panic!("Expected WASM output"),
    };
    
    println!("✓ WASIX multiple prints compilation successful");
    validate_wasix_imports(wasm_bytes).expect("WASIX import validation failed");
}

/// Validate that the WASM module contains proper WASIX imports
fn validate_wasix_imports(wasm_bytes: &[u8]) -> Result<(), String> {
    use wasmparser::{Parser, Payload};
    
    let mut has_wasix_imports = false;
    let mut fd_write_found = false;
    
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        match payload.map_err(|e| format!("WASM parsing error: {}", e))? {
            Payload::ImportSection(import_section) => {
                for import in import_section {
                    let import = import.map_err(|e| format!("Import parsing error: {}", e))?;
                    
                    println!("  Import: {} :: {}", import.module, import.name);
                    
                    // Check for WASIX imports
                    if import.module == "wasix_32v1" {
                        has_wasix_imports = true;
                        
                        if import.name == "fd_write" {
                            fd_write_found = true;
                            println!("    ✓ Found WASIX fd_write import");
                        }
                    }
                    
                    // Also check for legacy beanstalk_io imports (should be replaced)
                    if import.module == "beanstalk_io" && import.name == "print" {
                        println!("    ⚠ Found legacy beanstalk_io::print import (should be WASIX)");
                    }
                }
            }
            _ => {}
        }
    }
    
    if !has_wasix_imports {
        return Err("No WASIX imports found in WASM module".to_string());
    }
    
    if !fd_write_found {
        return Err("WASIX fd_write import not found".to_string());
    }
    
    Ok(())
}

/// Test WASIX WASM module validation
#[test]
fn test_wasix_wasm_module_validation() {
    let test_path = Path::new("tests/cases/wasix_print_basic.bst");
    
    // Build the project
    let project = build::build_project_files(test_path, false, &[])
        .expect("Compilation should succeed");
    
    // Extract WASM bytes
    let wasm_bytes = match &project.output_files[0] {
        OutputFile::Wasm(bytes) => bytes,
        _ => panic!("Expected WASM output"),
    };
    
    // Validate WASM module structure
    validate_wasm_module_structure(wasm_bytes)
        .expect("WASM module validation failed");
    
    println!("✓ WASIX WASM module validation successful");
}

/// Validate the overall WASM module structure
fn validate_wasm_module_structure(wasm_bytes: &[u8]) -> Result<(), String> {
    use wasmparser::{Parser, Payload};
    
    let mut has_type_section = false;
    let mut has_import_section = false;
    let mut has_function_section = false;
    let mut has_code_section = false;
    
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        match payload.map_err(|e| format!("WASM parsing error: {}", e))? {
            Payload::TypeSection(_) => has_type_section = true,
            Payload::ImportSection(_) => has_import_section = true,
            Payload::FunctionSection(_) => has_function_section = true,
            Payload::CodeSectionStart { .. } => has_code_section = true,
            _ => {}
        }
    }
    
    if !has_type_section {
        return Err("WASM module missing type section".to_string());
    }
    
    if !has_import_section {
        return Err("WASM module missing import section".to_string());
    }
    
    if !has_function_section {
        return Err("WASM module missing function section".to_string());
    }
    
    if !has_code_section {
        return Err("WASM module missing code section".to_string());
    }
    
    println!("✓ WASM module has all required sections");
    Ok(())
}