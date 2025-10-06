//! # Code Analysis Runner
//!
//! This module runs the comprehensive code usage analysis and generates
//! detailed reports for the backend cleanup task.

use crate::compiler_tests::code_usage_analyzer::{CodeUsageAnalyzer, CodeAnalysisResult};
use std::fs;
use std::path::Path;

/// Run the complete code analysis and generate reports
pub fn run_comprehensive_analysis() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Starting comprehensive code usage analysis...");
    
    let mut analyzer = CodeUsageAnalyzer::new();
    
    // Run the analysis
    let analysis_result = analyzer.analyze_usage()?;
    
    // Generate the report
    let report = analyzer.generate_report(&analysis_result);
    
    // Save report to file
    fs::write("code_usage_analysis_report.md", &report)?;
    
    // Print summary to console
    print_analysis_summary(&analysis_result);
    
    // Generate detailed function-level analysis
    generate_detailed_function_analysis()?;
    
    println!("✅ Analysis complete! Report saved to 'code_usage_analysis_report.md'");
    
    Ok(())
}

/// Print a summary of the analysis to the console
fn print_analysis_summary(analysis: &CodeAnalysisResult) {
    println!("\n📊 ANALYSIS SUMMARY");
    println!("==================");
    
    println!("\n🗑️  DEAD CODE (Remove Immediately):");
    for candidate in &analysis.removal_candidates {
        println!("   - {}", candidate);
    }
    
    println!("\n🔧 INCOMPLETE INTEGRATIONS (Implement Missing Integration):");
    for candidate in &analysis.implementation_candidates {
        println!("   - {}", candidate);
    }
    
    println!("\n⚡ OPTIMIZATION CODE (Remove - Not MIR's Purpose):");
    for candidate in &analysis.optimization_removal_candidates {
        println!("   - {}", candidate);
    }
    
    println!("\n📈 STATISTICS:");
    println!("   - Total modules: {}", analysis.modules.len());
    println!("   - Dead code files: {}", analysis.removal_candidates.len());
    println!("   - Incomplete integrations: {}", analysis.implementation_candidates.len());
    println!("   - Optimization code: {}", analysis.optimization_removal_candidates.len());
}

/// Generate detailed function-level analysis by examining specific files
fn generate_detailed_function_analysis() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔬 Generating detailed function-level analysis...");
    
    let mut detailed_report = String::new();
    detailed_report.push_str("# Detailed Function-Level Analysis\n\n");
    
    // Analyze specific MIR files in detail
    analyze_mir_file_details(&mut detailed_report, "src/compiler/mir/arena.rs", "Arena Allocation")?;
    analyze_mir_file_details(&mut detailed_report, "src/compiler/mir/cfg.rs", "Control Flow Graph")?;
    analyze_mir_file_details(&mut detailed_report, "src/compiler/mir/dataflow.rs", "Dataflow Analysis")?;
    analyze_mir_file_details(&mut detailed_report, "src/compiler/mir/liveness.rs", "Liveness Analysis")?;
    analyze_mir_file_details(&mut detailed_report, "src/compiler/mir/extract.rs", "MIR Extraction")?;
    
    // Analyze codegen files
    analyze_codegen_file_details(&mut detailed_report, "src/compiler/codegen/build_wasm.rs", "WASM Generation")?;
    analyze_codegen_file_details(&mut detailed_report, "src/compiler/codegen/wasm_encoding.rs", "WASM Encoding")?;
    
    // Save detailed report
    fs::write("detailed_function_analysis.md", detailed_report)?;
    
    Ok(())
}

/// Analyze a specific MIR file for function usage and categorization
fn analyze_mir_file_details(
    report: &mut String,
    file_path: &str,
    module_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(file_path).exists() {
        report.push_str(&format!("## {} ({})\n\n❌ File not found\n\n", module_name, file_path));
        return Ok(());
    }
    
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    
    report.push_str(&format!("## {} ({})\n\n", module_name, file_path));
    
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut imports = Vec::new();
    
    for line in lines {
        let trimmed = line.trim();
        
        // Extract function definitions
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
            if let Some(func_name) = extract_function_name(trimmed) {
                functions.push(func_name);
            }
        }
        
        // Extract struct definitions
        if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
            if let Some(struct_name) = extract_struct_name(trimmed) {
                structs.push(struct_name);
            }
        }
        
        // Extract imports
        if trimmed.starts_with("use ") {
            imports.push(trimmed.to_string());
        }
    }
    
    // Categorize based on MIR's purposes
    let category = categorize_mir_module(module_name, &functions, &content);
    
    report.push_str(&format!("**Category**: {}\n\n", category));
    
    if !functions.is_empty() {
        report.push_str("**Functions**:\n");
        for func in functions {
            let usage_status = determine_function_usage(&func, &content);
            report.push_str(&format!("- `{}` - {}\n", func, usage_status));
        }
        report.push_str("\n");
    }
    
    if !structs.is_empty() {
        report.push_str("**Structs**:\n");
        for struct_name in structs {
            report.push_str(&format!("- `{}`\n", struct_name));
        }
        report.push_str("\n");
    }
    
    report.push_str("---\n\n");
    
    Ok(())
}

/// Analyze a specific codegen file for function usage and categorization
fn analyze_codegen_file_details(
    report: &mut String,
    file_path: &str,
    module_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(file_path).exists() {
        report.push_str(&format!("## {} ({})\n\n❌ File not found\n\n", module_name, file_path));
        return Ok(());
    }
    
    let content = fs::read_to_string(file_path)?;
    
    report.push_str(&format!("## {} ({})\n\n", module_name, file_path));
    
    // Analyze for WASM-specific functionality
    let wasm_functions = count_wasm_related_functions(&content);
    let validation_functions = count_validation_functions(&content);
    let unused_functions = identify_unused_functions(&content);
    
    report.push_str(&format!("**WASM-related functions**: {}\n", wasm_functions));
    report.push_str(&format!("**Validation functions**: {}\n", validation_functions));
    report.push_str(&format!("**Potentially unused functions**: {}\n\n", unused_functions.len()));
    
    if !unused_functions.is_empty() {
        report.push_str("**Unused functions to investigate**:\n");
        for func in unused_functions {
            report.push_str(&format!("- `{}`\n", func));
        }
        report.push_str("\n");
    }
    
    report.push_str("---\n\n");
    
    Ok(())
}

/// Extract function name from a function definition line
fn extract_function_name(line: &str) -> Option<String> {
    if let Some(fn_pos) = line.find("fn ") {
        let after_fn = &line[fn_pos + 3..];
        if let Some(paren_pos) = after_fn.find('(') {
            let func_name = after_fn[..paren_pos].trim();
            if !func_name.is_empty() {
                return Some(func_name.to_string());
            }
        }
    }
    None
}

/// Extract struct name from a struct definition line
fn extract_struct_name(line: &str) -> Option<String> {
    if let Some(struct_pos) = line.find("struct ") {
        let after_struct = &line[struct_pos + 7..];
        let struct_name = after_struct.split_whitespace().next()?;
        if !struct_name.is_empty() {
            return Some(struct_name.to_string());
        }
    }
    None
}

/// Categorize a MIR module based on its purpose
fn categorize_mir_module(module_name: &str, _functions: &[String], content: &str) -> &'static str {
    match module_name {
        "Arena Allocation" => {
            "🔴 OPTIMIZATION CODE - Remove (arena allocation is optimization-focused, not core to MIR's purposes)"
        }
        "Control Flow Graph" => {
            if content.contains("optimization") || content.contains("transform") {
                "🟡 OPTIMIZATION CODE - Investigate (CFG may be used for optimization rather than borrow checking)"
            } else {
                "🟢 BORROW CHECKING - Keep (CFG needed for precise borrow analysis)"
            }
        }
        "Dataflow Analysis" => {
            "🟡 OPTIMIZATION CODE - Investigate (dataflow analysis is typically optimization-focused)"
        }
        "Liveness Analysis" => {
            "🟡 OPTIMIZATION CODE - Investigate (liveness analysis is typically optimization-focused)"
        }
        "MIR Extraction" => {
            "🟡 UNCLEAR PURPOSE - Investigate (need to determine if this is essential or utility)"
        }
        _ => "🟢 CORE FUNCTIONALITY - Keep"
    }
}

/// Determine if a function appears to be used based on simple heuristics
fn determine_function_usage(func_name: &str, content: &str) -> &'static str {
    let call_count = content.matches(func_name).count();
    
    if call_count <= 1 {
        "❌ Potentially unused"
    } else if call_count <= 3 {
        "⚠️ Limited usage"
    } else {
        "✅ Actively used"
    }
}

/// Count WASM-related functions in codegen
fn count_wasm_related_functions(content: &str) -> usize {
    content.lines()
        .filter(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")) &&
            (trimmed.contains("wasm") || trimmed.contains("Wasm") || trimmed.contains("WASM"))
        })
        .count()
}

/// Count validation functions in codegen
fn count_validation_functions(content: &str) -> usize {
    content.lines()
        .filter(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")) &&
            (trimmed.contains("valid") || trimmed.contains("check") || trimmed.contains("verify"))
        })
        .count()
}

/// Identify potentially unused functions in codegen
fn identify_unused_functions(content: &str) -> Vec<String> {
    let mut functions = Vec::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            if let Some(func_name) = extract_function_name(trimmed) {
                // Simple heuristic: if function name appears only once, it might be unused
                let occurrences = content.matches(&func_name).count();
                if occurrences <= 1 {
                    functions.push(func_name);
                }
            }
        }
    }
    
    functions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name_extraction() {
        assert_eq!(
            extract_function_name("pub fn test_function() -> Result<(), Error> {"),
            Some("test_function".to_string())
        );
        
        assert_eq!(
            extract_function_name("fn simple() {"),
            Some("simple".to_string())
        );
    }

    #[test]
    fn test_struct_name_extraction() {
        assert_eq!(
            extract_struct_name("pub struct TestStruct {"),
            Some("TestStruct".to_string())
        );
        
        assert_eq!(
            extract_struct_name("struct Simple;"),
            Some("Simple".to_string())
        );
    }
}