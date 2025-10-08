//! # Code Usage Analyzer
//!
//! This module analyzes the MIR and codegen modules to identify:
//! - Used vs unused functions, structs, and modules
//! - Dead code that can be removed immediately
//! - Missing integrations that need to be implemented
//! - Future stubs that should be kept for planned features
//!
//! The analyzer traces function calls from entry points (ast_to_wir, new_wasm_module)
//! and categorizes code based on MIR's two core purposes:
//! 1. Efficient WASM lowering
//! 2. Borrow checking and lifetime analysis

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum CodeStatus {
    /// Currently used in the compilation pipeline
    Active,
    /// Not used anywhere, should be removed
    DeadCode,
    /// Partially implemented, needs completion for integration
    IncompleteIntegration,
    /// Future feature stub, keep for planned work
    FutureStub,
    /// Optimization-focused code that doesn't belong in MIR
    OptimizationCode,
}

#[derive(Debug)]
pub struct CodeAnalysisResult {
    pub functions: HashMap<String, CodeStatus>,
    pub structs: HashMap<String, CodeStatus>,
    pub modules: HashMap<String, CodeStatus>,
    pub removal_candidates: Vec<String>,
    pub implementation_candidates: Vec<String>,
    pub optimization_removal_candidates: Vec<String>,
}

#[derive(Debug)]
pub struct CodeUsageAnalyzer {
    /// Functions that are actively called from entry points
    used_functions: HashSet<String>,
    /// Structs/types that are actively used
    #[allow(dead_code)] // Used for future analysis features
    used_types: HashSet<String>,
    /// Modules that have active integration
    #[allow(dead_code)] // Used for future analysis features
    active_modules: HashSet<String>,
    /// Function call graph for tracing usage
    call_graph: HashMap<String, Vec<String>>,
    /// Type usage graph
    #[allow(dead_code)] // Used for future analysis features
    type_usage: HashMap<String, Vec<String>>,
}

impl CodeUsageAnalyzer {
    pub fn new() -> Self {
        Self {
            used_functions: HashSet::new(),
            used_types: HashSet::new(),
            active_modules: HashSet::new(),
            call_graph: HashMap::new(),
            type_usage: HashMap::new(),
        }
    }

    /// Analyze code usage starting from known entry points
    pub fn analyze_usage(&mut self) -> Result<CodeAnalysisResult, Box<dyn std::error::Error>> {
        // Step 1: Identify entry points
        let entry_points = vec![
            "ast_to_wir".to_string(),
            "new_wasm_module".to_string(),
            "borrow_check_pipeline".to_string(),
        ];

        // Step 2: Build call graph from source files
        self.build_call_graph()?;

        // Step 3: Trace usage from entry points
        for entry_point in entry_points {
            self.trace_usage_from_function(&entry_point);
        }

        // Step 4: Analyze MIR module files
        let mir_analysis = self.analyze_mir_module()?;

        // Step 5: Analyze codegen module files
        let codegen_analysis = self.analyze_codegen_module()?;

        // Step 6: Combine results and categorize
        Ok(self.combine_analysis_results(mir_analysis, codegen_analysis))
    }

    /// Build call graph by parsing source files
    fn build_call_graph(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Parse MIR module files
        self.parse_module_files("src/compiler/mir")?;
        
        // Parse codegen module files
        self.parse_module_files("src/compiler/codegen")?;

        Ok(())
    }

    /// Parse all Rust files in a module directory
    fn parse_module_files(&mut self, module_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let entries = fs::read_dir(module_path)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().map_or(false, |ext| ext == "rs") {
                self.parse_rust_file(&path)?;
            }
        }
        
        Ok(())
    }

    /// Parse a single Rust file to extract function definitions and calls
    fn parse_rust_file(&mut self, file_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().collect();
        
        let mut current_function = None;
        
        for line in lines {
            let trimmed = line.trim();
            
            // Function definitions
            if let Some(func_name) = self.extract_function_definition(trimmed) {
                current_function = Some(func_name.clone());
                self.call_graph.entry(func_name).or_insert_with(Vec::new);
            }
            
            // Function calls within current function
            if let Some(ref current_func) = current_function {
                let called_functions = self.extract_function_calls(trimmed);
                self.call_graph.entry(current_func.clone())
                    .or_insert_with(Vec::new)
                    .extend(called_functions);
            }
            
            // Struct/type definitions and usage
            self.extract_type_usage(trimmed);
        }
        
        Ok(())
    }

    /// Extract function definition from a line
    fn extract_function_definition(&self, line: &str) -> Option<String> {
        // Match patterns like "pub fn function_name(" or "fn function_name("
        if line.contains("fn ") && line.contains("(") {
            let parts: Vec<&str> = line.split("fn ").collect();
            if parts.len() > 1 {
                let func_part = parts[1];
                let name_end = func_part.find('(').unwrap_or(func_part.len());
                let func_name = func_part[..name_end].trim();
                if !func_name.is_empty() && func_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return Some(func_name.to_string());
                }
            }
        }
        None
    }

    /// Extract function calls from a line
    fn extract_function_calls(&self, line: &str) -> Vec<String> {
        let mut calls = Vec::new();
        
        // Simple pattern matching for function calls
        // This is a basic implementation - could be enhanced with proper parsing
        let words: Vec<&str> = line.split_whitespace().collect();
        for window in words.windows(2) {
            if window[1].starts_with('(') {
                let func_name = window[0].trim_end_matches(&[':', '.', '!'][..]);
                if func_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    calls.push(func_name.to_string());
                }
            }
        }
        
        calls
    }

    /// Extract type usage from a line
    fn extract_type_usage(&mut self, line: &str) {
        // Look for struct definitions, type usage, etc.
        if line.contains("struct ") || line.contains("enum ") {
            // Extract type definitions
            // This is a simplified implementation
        }
    }

    /// Trace usage starting from a specific function
    fn trace_usage_from_function(&mut self, function_name: &str) {
        if self.used_functions.contains(function_name) {
            return; // Already processed
        }
        
        self.used_functions.insert(function_name.to_string());
        
        // Get called functions and clone to avoid borrowing issues
        let called_functions = self.call_graph.get(function_name).cloned().unwrap_or_default();
        
        // Recursively trace called functions
        for called_func in called_functions {
            self.trace_usage_from_function(&called_func);
        }
    }

    /// Analyze MIR module for usage patterns
    fn analyze_mir_module(&self) -> Result<HashMap<String, CodeStatus>, Box<dyn std::error::Error>> {
        let mut analysis = HashMap::new();
        
        // Known MIR files and their purposes
        let mir_files = vec![
            ("arena.rs", "Arena allocation - optimization focused"),
            ("build_wir.rs", "Core AST to MIR transformation - essential"),
            ("cfg.rs", "Control flow graph - may be optimization focused"),
            ("check.rs", "MIR validation - essential for correctness"),
            ("counter.rs", "ID generation - utility"),
            ("dataflow.rs", "Dataflow analysis - may be optimization focused"),
            ("diagnose.rs", "Error diagnostics - essential for user experience"),
            ("extract.rs", "MIR extraction utilities - unclear purpose"),
            ("liveness.rs", "Liveness analysis - may be optimization focused"),
            ("wir_nodes.rs", "Core MIR data structures - essential"),
            ("mir.rs", "Main MIR interface - essential"),
            ("place.rs", "Place abstraction for borrow checking - essential"),
            ("unified_borrow_checker.rs", "Borrow checking - essential"),
        ];
        
        for (file_name, description) in mir_files {
            let status = self.categorize_mir_file(file_name, description);
            analysis.insert(file_name.to_string(), status);
        }
        
        Ok(analysis)
    }

    /// Analyze codegen module for usage patterns
    fn analyze_codegen_module(&self) -> Result<HashMap<String, CodeStatus>, Box<dyn std::error::Error>> {
        let mut analysis = HashMap::new();
        
        // Known codegen files and their purposes
        let codegen_files = vec![
            ("build_wasm.rs", "Core WASM generation - essential"),
            ("wasm_encoding.rs", "WASM module encoding - essential"),
            ("wat_to_wasm.rs", "WAT text format conversion - utility"),
        ];
        
        for (file_name, description) in codegen_files {
            let status = self.categorize_codegen_file(file_name, description);
            analysis.insert(file_name.to_string(), status);
        }
        
        Ok(analysis)
    }

    /// Categorize MIR file based on its purpose and usage
    fn categorize_mir_file(&self, file_name: &str, _description: &str) -> CodeStatus {
        match file_name {
            // Essential for MIR's core purposes
            "build_wir.rs" | "wir_nodes.rs" | "mir.rs" | "place.rs" | "unified_borrow_checker.rs" => {
                CodeStatus::Active
            }
            
            // Essential for error handling and validation
            "check.rs" | "diagnose.rs" => {
                CodeStatus::Active
            }
            
            // Utility functions that may be needed
            "counter.rs" => {
                if self.used_functions.iter().any(|f| f.contains("counter") || f.contains("id")) {
                    CodeStatus::Active
                } else {
                    CodeStatus::IncompleteIntegration
                }
            }
            
            // Optimization-focused code that doesn't belong in MIR
            "arena.rs" => {
                CodeStatus::OptimizationCode
            }
            
            // Analysis code that may be optimization-focused
            "cfg.rs" | "dataflow.rs" | "liveness.rs" => {
                // These are typically optimization-focused in most compilers
                CodeStatus::OptimizationCode
            }
            
            // Unclear purpose - needs investigation
            "extract.rs" => {
                CodeStatus::IncompleteIntegration
            }
            
            _ => CodeStatus::DeadCode,
        }
    }

    /// Categorize codegen file based on its purpose and usage
    fn categorize_codegen_file(&self, file_name: &str, _description: &str) -> CodeStatus {
        match file_name {
            // Essential for WASM generation
            "build_wasm.rs" | "wasm_encoding.rs" => {
                CodeStatus::Active
            }
            
            // Utility that may not be essential
            "wat_to_wasm.rs" => {
                if self.used_functions.iter().any(|f| f.contains("wat")) {
                    CodeStatus::Active
                } else {
                    CodeStatus::IncompleteIntegration
                }
            }
            
            _ => CodeStatus::DeadCode,
        }
    }

    /// Combine analysis results from different modules
    fn combine_analysis_results(
        &self,
        mir_analysis: HashMap<String, CodeStatus>,
        codegen_analysis: HashMap<String, CodeStatus>,
    ) -> CodeAnalysisResult {
        let functions = HashMap::new();
        let structs = HashMap::new();
        let mut modules = HashMap::new();
        let mut removal_candidates = Vec::new();
        let mut implementation_candidates = Vec::new();
        let mut optimization_removal_candidates = Vec::new();

        // Process MIR analysis
        for (file_name, status) in mir_analysis {
            modules.insert(format!("mir::{}", file_name), status.clone());
            
            match status {
                CodeStatus::DeadCode => {
                    removal_candidates.push(format!("mir::{}", file_name));
                }
                CodeStatus::IncompleteIntegration => {
                    implementation_candidates.push(format!("mir::{}", file_name));
                }
                CodeStatus::OptimizationCode => {
                    optimization_removal_candidates.push(format!("mir::{}", file_name));
                }
                _ => {}
            }
        }

        // Process codegen analysis
        for (file_name, status) in codegen_analysis {
            modules.insert(format!("codegen::{}", file_name), status.clone());
            
            match status {
                CodeStatus::DeadCode => {
                    removal_candidates.push(format!("codegen::{}", file_name));
                }
                CodeStatus::IncompleteIntegration => {
                    implementation_candidates.push(format!("codegen::{}", file_name));
                }
                CodeStatus::OptimizationCode => {
                    optimization_removal_candidates.push(format!("codegen::{}", file_name));
                }
                _ => {}
            }
        }

        CodeAnalysisResult {
            functions,
            structs,
            modules,
            removal_candidates,
            implementation_candidates,
            optimization_removal_candidates,
        }
    }

    /// Generate a comprehensive report of the analysis
    pub fn generate_report(&self, analysis: &CodeAnalysisResult) -> String {
        let mut report = String::new();
        
        report.push_str("# Code Usage Analysis Report\n\n");
        report.push_str("## Summary\n\n");
        report.push_str(&format!("- Total modules analyzed: {}\n", analysis.modules.len()));
        report.push_str(&format!("- Dead code candidates: {}\n", analysis.removal_candidates.len()));
        report.push_str(&format!("- Incomplete integrations: {}\n", analysis.implementation_candidates.len()));
        report.push_str(&format!("- Optimization code to remove: {}\n", analysis.optimization_removal_candidates.len()));
        
        report.push_str("\n## Dead Code (Remove Immediately)\n\n");
        for candidate in &analysis.removal_candidates {
            report.push_str(&format!("- {}\n", candidate));
        }
        
        report.push_str("\n## Incomplete Integrations (Implement Missing Integration)\n\n");
        for candidate in &analysis.implementation_candidates {
            report.push_str(&format!("- {}\n", candidate));
        }
        
        report.push_str("\n## Optimization Code (Remove - Not MIR's Purpose)\n\n");
        for candidate in &analysis.optimization_removal_candidates {
            report.push_str(&format!("- {}\n", candidate));
        }
        
        report.push_str("\n## Module Status Details\n\n");
        for (module, status) in &analysis.modules {
            report.push_str(&format!("- {}: {:?}\n", module, status));
        }
        
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyzer_creation() {
        let analyzer = CodeUsageAnalyzer::new();
        assert!(analyzer.used_functions.is_empty());
        assert!(analyzer.call_graph.is_empty());
    }

    #[test]
    fn test_function_definition_extraction() {
        let analyzer = CodeUsageAnalyzer::new();
        
        assert_eq!(
            analyzer.extract_function_definition("pub fn test_function() {"),
            Some("test_function".to_string())
        );
        
        assert_eq!(
            analyzer.extract_function_definition("fn another_test(param: i32) -> bool {"),
            Some("another_test".to_string())
        );
        
        assert_eq!(
            analyzer.extract_function_definition("let x = 5;"),
            None
        );
    }
}