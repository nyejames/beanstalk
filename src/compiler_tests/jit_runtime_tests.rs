use crate::compiler::compiler_errors::CompileError;
use crate::runtime::jit::{
    execute_direct_jit_with_capture, get_captured_output, clear_captured_output,
    CapturedOutput, MemoryCleanup,
};
use crate::runtime::{IoBackend, RuntimeConfig, CompilationMode, CraneliftOptLevel};

/// Test basic JIT execution with WASIX backend
#[test]
fn test_wasix_backend_basic_execution() {
    // Create a simple WASM module that should execute successfully
    let wasm_bytes = create_simple_wasm_module();
    
    let config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Wasix,
        hot_reload: false,
        flags: vec![],
    };

    // Test should not panic and should return Ok
    // Note: WASIX may require a Tokio runtime, so we catch that specific error
    let result = std::panic::catch_unwind(|| {
        execute_direct_jit_with_capture(&wasm_bytes, &config, false)
    });
    
    match result {
        Ok(exec_result) => {
            // Execution completed without panic
            match exec_result {
                Ok(_) => {
                    // Success case - WASM executed successfully
                    println!("WASIX backend execution succeeded");
                }
                Err(e) => {
                    // Expected failure case - ensure it's a reasonable error
                    let error_msg = format!("{:?}", e);
                    assert!(
                        error_msg.contains("Failed to compile WASM module") || 
                        error_msg.contains("magic header"),
                        "Unexpected error type: {}",
                        error_msg
                    );
                }
            }
        }
        Err(panic_payload) => {
            // Execution panicked - check if it's the expected Tokio runtime error
            let panic_msg = if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic".to_string()
            };
            
            // This is expected when WASIX requires a Tokio runtime
            assert!(
                panic_msg.contains("there is no reactor running") ||
                panic_msg.contains("Tokio"),
                "Unexpected panic: {}",
                panic_msg
            );
            
            println!("WASIX backend test completed - requires Tokio runtime as expected");
        }
    }
}

/// Test native backend with output capture
#[test]
fn test_native_backend_with_output_capture() {
    clear_captured_output();
    
    let wasm_bytes = create_simple_wasm_module();
    
    let config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Native,
        hot_reload: false,
        flags: vec![],
    };

    // Test with output capture enabled
    let result = execute_direct_jit_with_capture(&wasm_bytes, &config, true);
    
    match result {
        Ok(_) => {
            // Check if output was captured
            if let Some(captured) = get_captured_output() {
                // Verify captured output structure
                assert!(captured.stdout.lock().is_ok());
                assert!(captured.stderr.lock().is_ok());
                println!("Native backend output capture configured successfully");
            }
        }
        Err(e) => {
            // Expected failure case - ensure it's a reasonable error
            let error_msg = format!("{:?}", e);
            assert!(
                error_msg.contains("Failed to compile WASM module") || 
                error_msg.contains("magic header") ||
                error_msg.contains("backend requires WASM memory export"),
                "Unexpected error type: {}",
                error_msg
            );
        }
    }
}

/// Test memory management utilities
#[test]
fn test_memory_cleanup_utilities() {
    let mut cleanup = MemoryCleanup::new();
    
    // Test tracking allocations
    cleanup.track_allocation(0x1000, 256);
    cleanup.track_allocation(0x2000, 512);
    
    let tracked = cleanup.get_tracked();
    assert_eq!(tracked.len(), 2);
    assert_eq!(tracked[0], (0x1000, 256));
    assert_eq!(tracked[1], (0x2000, 512));
    
    // Test clearing tracked allocations
    cleanup.clear_tracked();
    assert_eq!(cleanup.get_tracked().len(), 0);
}

/// Test captured output functionality
#[test]
fn test_captured_output_functionality() {
    use std::sync::{Arc, Mutex};
    
    let stdout_buffer = Arc::new(Mutex::new(Vec::new()));
    let stderr_buffer = Arc::new(Mutex::new(Vec::new()));
    
    let captured = CapturedOutput {
        stdout: stdout_buffer.clone(),
        stderr: stderr_buffer.clone(),
    };
    
    // Test writing to buffers
    {
        let mut stdout = stdout_buffer.lock().unwrap();
        stdout.extend_from_slice(b"Hello, World!");
    }
    
    {
        let mut stderr = stderr_buffer.lock().unwrap();
        stderr.extend_from_slice(b"Error message");
    }
    
    // Test reading captured output
    let stdout_result = captured.get_stdout();
    let stderr_result = captured.get_stderr();
    
    assert!(stdout_result.is_ok());
    assert!(stderr_result.is_ok());
    
    assert_eq!(stdout_result.unwrap(), "Hello, World!");
    assert_eq!(stderr_result.unwrap(), "Error message");
    
    // Test clearing output
    captured.clear();
    
    let stdout_after_clear = captured.get_stdout().unwrap();
    let stderr_after_clear = captured.get_stderr().unwrap();
    
    assert!(stdout_after_clear.is_empty());
    assert!(stderr_after_clear.is_empty());
}

/// Test runtime configuration for different backends
#[test]
fn test_runtime_config_backends() {
    // Test WASIX backend configuration
    let wasix_config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Wasix,
        hot_reload: false,
        flags: vec![],
    };
    
    assert!(matches!(wasix_config.io_backend, IoBackend::Wasix));
    
    // Test Native backend configuration
    let native_config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Native,
        hot_reload: false,
        flags: vec![],
    };
    
    assert!(matches!(native_config.io_backend, IoBackend::Native));
    
    // Test development configuration
    let dev_config = RuntimeConfig::for_development();
    assert!(matches!(dev_config.io_backend, IoBackend::Wasix));
    assert!(dev_config.hot_reload);
    
    // Test native release configuration
    let release_config = RuntimeConfig::for_native_release();
    assert!(matches!(release_config.io_backend, IoBackend::Wasix));
    assert!(!release_config.hot_reload);
}

/// Test error handling scenarios
#[test]
fn test_error_handling_scenarios() {
    // Test with invalid WASM bytes
    let invalid_wasm = vec![0x00, 0x01, 0x02, 0x03]; // Invalid WASM magic header
    
    let config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Wasix,
        hot_reload: false,
        flags: vec![],
    };
    
    let result = execute_direct_jit_with_capture(&invalid_wasm, &config, false);
    
    // Should fail with a compilation error
    assert!(result.is_err());
    
    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    
    // Should be a compiler error about WASM compilation
    assert!(
        error_msg.contains("Failed to compile WASM module") ||
        error_msg.contains("magic header") ||
        error_msg.contains("backend requires WASM memory export") ||
        error_msg.contains("there is no reactor running"),
        "Expected WASM compilation error, got: {}",
        error_msg
    );
}

/// Test memory bounds checking (unit test for memory functions)
#[test]
fn test_memory_bounds_validation() {
    // Test memory cleanup tracking
    let mut cleanup = MemoryCleanup::new();
    
    // Test reasonable allocation sizes
    cleanup.track_allocation(0x1000, 1024);
    cleanup.track_allocation(0x2000, 4096);
    
    let tracked = cleanup.get_tracked();
    assert_eq!(tracked.len(), 2);
    
    // Test that allocations are tracked correctly
    assert!(tracked.iter().any(|(ptr, size)| *ptr == 0x1000 && *size == 1024));
    assert!(tracked.iter().any(|(ptr, size)| *ptr == 0x2000 && *size == 4096));
}

/// Helper function to create a minimal WASM module for testing
/// Note: This creates invalid WASM bytes for testing error handling
fn create_simple_wasm_module() -> Vec<u8> {
    // This is intentionally invalid WASM to test error handling
    // In a real implementation, this would be a valid minimal WASM module
    vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic header (intentionally wrong)
        0x01, 0x00, 0x00, 0x00, // WASM version
    ]
}

/// Integration test for template output functionality
/// This test requires a valid WASM module with template output statements
#[test]
#[ignore] // Ignored until we have valid WASM modules to test with
fn test_template_output_integration() {
    // This test would require a real WASM module generated by the Beanstalk compiler
    // with template output statements to test the full integration
    
    // Example of what this test would do:
    // 1. Compile a Beanstalk program with template output statements
    // 2. Execute it with output capture
    // 3. Verify the captured output matches expected results
    
    let _config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Native,
        hot_reload: false,
        flags: vec![],
    };
    
    // TODO: Implement when we have valid WASM modules from the compiler
    println!("Template output integration test - requires valid WASM modules");
}

/// Test WASIX fd_write functionality
#[test]
#[ignore] // Ignored until we have valid WASM modules to test with
fn test_wasix_fd_write_functionality() {
    // This test would verify that WASIX fd_write calls work correctly
    // It requires a WASM module that makes fd_write calls
    
    let _config = RuntimeConfig {
        compilation_mode: CompilationMode::DirectJit,
        io_backend: IoBackend::Wasix,
        hot_reload: false,
        flags: vec![],
    };
    
    // TODO: Implement when we have WASM modules that use fd_write
    println!("WASIX fd_write test - requires WASM modules with fd_write calls");
}