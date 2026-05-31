//! HTML JavaScript `@bst.*` annotation parser.
//!
//! WHAT: turns a single JS source file into a `ParsedJsLibrary` containing opaque types,
//!       free functions, receiver methods, registered runtime imports, and diagnostics.
//! WHY: this parser stays independent from compiler diagnostics and package registration so
//!      project-local JS providers and built-in JS-backed packages can share one source model.
//!
//! ## Module layout
//!
//! - `parsed_js_library`: parser-owned data model (spans, diagnostics, signatures).
//! - `comment_extractor`: finds `/** ... */` blocks and extracts `@bst.opaque` / `@bst.sig`.
//! - `export_scanner`: finds supported JS exports and rejects unsupported forms.
//! - `signature_parser`: parses the Beanstalk parameter/return syntax inside `@bst.sig`.
//! - `mod.rs` (this file): orchestrates extraction → scanning → binding → signature parsing.

mod comment_extractor;
mod export_scanner;
pub(crate) mod parsed_js_library;
mod signature_parser;

#[cfg(test)]
mod tests;

use comment_extractor::{AnnotationKind, ExtractedAnnotation, extract_annotations};
use export_scanner::{JsExport, scan_exports};
use parsed_js_library::{
    JsDiagnosticKind, JsParserDiagnostic, ParsedJsFunction, ParsedJsLibrary, ParsedOpaqueType,
    ParsedRuntimeImport,
};
use signature_parser::{SignatureParseInput, parse_signature};

use crate::projects::html_project::external_js::runtime_module_registry::RuntimeModuleRegistry;
use std::collections::{BTreeMap, BTreeSet};

/// Parses a single JS source file into a `ParsedJsLibrary` using an explicit registry.
///
/// WHAT: extracts `@bst.*` annotations, scans for JS exports, matches each `@bst.sig`
///       to the immediately following supported export, parses signatures, and validates
///       arity and duplicate names against the provided runtime module registry.
///
/// This function does not interact with `ExternalPackageRegistry` or `CompilerDiagnostic`.
/// It returns parser-local data that package registration and provider code convert later.
pub(crate) fn parse_js_library(source: &str, registry: &RuntimeModuleRegistry) -> ParsedJsLibrary {
    let mut orchestrator = ParseOrchestrator::new(source, registry);
    orchestrator.run()
}

struct ParseOrchestrator<'a> {
    source: &'a str,
    registry: &'a RuntimeModuleRegistry,
    annotations: Vec<ExtractedAnnotation>,
    exports: Vec<JsExport>,
    diagnostics: Vec<JsParserDiagnostic>,
    seen_beanstalk_names: Vec<String>,
    seen_js_names: Vec<String>,
}

impl<'a> ParseOrchestrator<'a> {
    fn new(source: &'a str, registry: &'a RuntimeModuleRegistry) -> Self {
        Self {
            source,
            registry,
            annotations: Vec::new(),
            exports: Vec::new(),
            diagnostics: Vec::new(),
            seen_beanstalk_names: Vec::new(),
            seen_js_names: Vec::new(),
        }
    }

    fn run(&mut self) -> ParsedJsLibrary {
        // Phase 1: extract annotations from doc comments.
        let comment_result = extract_annotations(self.source);
        self.annotations = comment_result.annotations;
        self.diagnostics.extend(comment_result.diagnostics);

        // Phase 2: scan for JS exports against the injected runtime module registry.
        let export_result = scan_exports(self.source, self.registry);
        self.exports = export_result.exports;
        self.diagnostics.extend(export_result.diagnostics);

        // Phase 3: bind annotations to exports, parse signatures, validate.
        let mut library = self.bind_and_validate();

        // Deduplicate runtime imports deterministically by module specifier,
        // merging imported names across duplicate import statements.
        library.runtime_imports = deduplicate_runtime_imports(export_result.runtime_imports);

        library
    }

    // ------------------------
    //  Binding and validation
    // ------------------------

    fn bind_and_validate(&mut self) -> ParsedJsLibrary {
        let mut library = ParsedJsLibrary::empty();

        // Collect opaque types (file-level annotations, no export binding needed).
        for annotation in &self.annotations {
            if let AnnotationKind::Opaque { type_name } = &annotation.kind {
                if self.seen_beanstalk_names.contains(type_name) {
                    self.diagnostics.push(JsParserDiagnostic {
                        message: format!(
                            "Duplicate Beanstalk-facing name `{}` in JS library.",
                            type_name
                        ),
                        span: annotation.span.clone(),
                        kind: JsDiagnosticKind::DuplicateBeanstalkName,
                    });
                } else {
                    self.seen_beanstalk_names.push(type_name.clone());
                }

                library.opaque_types.push(ParsedOpaqueType {
                    name: type_name.clone(),
                    span: annotation.span.clone(),
                });
            }
        }

        // Bind `@bst.sig` annotations to the next supported JS export.
        let mut annotation_index = 0;
        let mut export_index = 0;

        while annotation_index < self.annotations.len() {
            let annotation = &self.annotations[annotation_index];
            annotation_index += 1;

            let (beanstalk_name, signature_text, annotation_span) = match &annotation.kind {
                AnnotationKind::Sig {
                    beanstalk_name,
                    signature_text,
                } => (
                    beanstalk_name.clone(),
                    signature_text.clone(),
                    annotation.span.clone(),
                ),
                AnnotationKind::Opaque { .. } => continue,
            };

            // Find the next supported export that comes after this annotation.
            let matched_index =
                self.find_next_export_after(annotation_span.byte_start, export_index);

            if let Some(index) = matched_index {
                let export = self.exports[index].clone();
                export_index = index + 1;

                // Check for duplicate JS export name
                if self.seen_js_names.contains(&export.js_name) {
                    self.diagnostics.push(JsParserDiagnostic {
                        message: format!(
                            "Duplicate JS export name `{}` in library.",
                            export.js_name
                        ),
                        span: export.span.clone(),
                        kind: JsDiagnosticKind::DuplicateJsExportName,
                    });
                } else {
                    self.seen_js_names.push(export.js_name.clone());
                }

                // Check for duplicate Beanstalk-facing name
                if self.seen_beanstalk_names.contains(&beanstalk_name) {
                    self.diagnostics.push(JsParserDiagnostic {
                        message: format!(
                            "Duplicate Beanstalk-facing name `{}` in JS library.",
                            beanstalk_name
                        ),
                        span: annotation_span.clone(),
                        kind: JsDiagnosticKind::DuplicateBeanstalkName,
                    });
                } else {
                    self.seen_beanstalk_names.push(beanstalk_name.clone());
                }

                // Parse the signature body
                let sig_result = parse_signature(SignatureParseInput {
                    text: signature_text.clone(),
                    base_byte: annotation_span.byte_start,
                    base_line: annotation_span.line,
                    base_column: annotation_span.column,
                });
                self.diagnostics.extend(sig_result.diagnostics);

                // Validate arity: Beanstalk ABI parameters vs JS parameters
                let abi_count = sig_result.signature.abi_parameter_count();
                if abi_count != export.parameter_count {
                    self.diagnostics.push(JsParserDiagnostic {
                        message: format!(
                            "Annotated JS export `{}` has {} Beanstalk ABI parameter(s) but {} JS parameter(s). \
                             Receiver `this` counts as the first JS parameter. Beanstalk JS library exports must use one plain JS parameter per Beanstalk ABI parameter.",
                            export.js_name,
                            abi_count,
                            export.parameter_count
                        ),
                        span: export.span.clone(),
                        kind: JsDiagnosticKind::ArityMismatch,
                    });
                }

                let parsed_function = ParsedJsFunction {
                    beanstalk_name: beanstalk_name.clone(),
                    js_name: export.js_name.clone(),
                    signature: sig_result.signature,
                    annotation_span: annotation_span.clone(),
                    export_span: export.span.clone(),
                };

                if parsed_function.signature.has_receiver() {
                    library.receiver_methods.push(parsed_function);
                } else {
                    library.free_functions.push(parsed_function);
                }
            } else {
                self.diagnostics.push(JsParserDiagnostic {
                    message: format!(
                        "`@bst.sig` for `{}` is not followed by a supported JS export declaration.",
                        beanstalk_name
                    ),
                    span: annotation_span,
                    kind: JsDiagnosticKind::MissingExportAfterSig,
                });
            }
        }

        // Report unannotated exports
        for export in &self.exports {
            let is_annotated = library
                .free_functions
                .iter()
                .any(|f| f.js_name == export.js_name)
                || library
                    .receiver_methods
                    .iter()
                    .any(|f| f.js_name == export.js_name);

            if !is_annotated {
                self.diagnostics.push(JsParserDiagnostic {
                    message: format!(
                        "JavaScript export `{}` is not annotated with `@bst.sig`. \
                         Every export in a Beanstalk JS library must be explicitly annotated. Keep private helpers unexported.",
                        export.js_name
                    ),
                    span: export.span.clone(),
                    kind: JsDiagnosticKind::UnannotatedExport,
                });
            }
        }

        self.validate_signature_type_names(&library);

        library.diagnostics = std::mem::take(&mut self.diagnostics);
        library
    }

    /// Finds the next supported JS export whose span starts after the given byte offset.
    fn find_next_export_after(&self, byte_offset: usize, start_index: usize) -> Option<usize> {
        for index in start_index..self.exports.len() {
            let export = &self.exports[index];
            if export.span.byte_start > byte_offset {
                return Some(index);
            }
        }
        None
    }

    fn validate_signature_type_names(&mut self, library: &ParsedJsLibrary) {
        let opaque_names = library
            .opaque_types
            .iter()
            .map(|opaque| opaque.name.as_str())
            .collect::<Vec<_>>();

        for function in library
            .free_functions
            .iter()
            .chain(library.receiver_methods.iter())
        {
            self.validate_function_signature_types(function, &opaque_names);
        }
    }

    fn validate_function_signature_types(
        &mut self,
        function: &ParsedJsFunction,
        opaque_names: &[&str],
    ) {
        if function.signature.has_unsupported_generic_parameters {
            return;
        }

        for parameter in &function.signature.parameters {
            self.validate_type_name(
                &parameter.type_name,
                &function.beanstalk_name,
                &function.annotation_span,
                opaque_names,
            );
        }

        for return_type in &function.signature.returns {
            self.validate_type_name(
                &return_type.type_name,
                &function.beanstalk_name,
                &function.annotation_span,
                opaque_names,
            );
        }
    }

    fn validate_type_name(
        &mut self,
        type_name: &str,
        function_name: &str,
        span: &parsed_js_library::JsSourceSpan,
        opaque_names: &[&str],
    ) {
        if !should_validate_known_type_name(type_name) {
            return;
        }

        if is_builtin_signature_type(type_name) || opaque_names.contains(&type_name) {
            return;
        }

        self.diagnostics.push(JsParserDiagnostic {
            message: format!(
                "Unknown external type `{}` in `@bst.sig` for `{}`. Declare it with `@bst.opaque {}` before using it in Beanstalk JS library signatures.",
                type_name, function_name, type_name
            ),
            span: span.clone(),
            kind: JsDiagnosticKind::UnknownExternalType,
        });
    }
}

fn is_builtin_signature_type(type_name: &str) -> bool {
    matches!(type_name, "Int" | "Float" | "Bool" | "String" | "Char")
}

fn should_validate_known_type_name(type_name: &str) -> bool {
    !type_name.is_empty()
        && !matches!(
            type_name,
            "Void" | "void" | "None" | "none" | "Unit" | "unit" | "()"
        )
        && type_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Deduplicate runtime imports by module specifier, merging imported names.
///
/// WHAT: collects all names imported from the same module into one entry.
/// WHY: parser may see duplicate import statements; the provider and backend
///      only need one `RequiredRuntimeImport` per module.
fn deduplicate_runtime_imports(imports: Vec<ParsedRuntimeImport>) -> Vec<ParsedRuntimeImport> {
    let mut by_module: BTreeMap<String, (BTreeSet<String>, parsed_js_library::JsSourceSpan)> =
        BTreeMap::new();

    for import in imports {
        let entry = by_module
            .entry(import.module_name)
            .or_insert_with(|| (BTreeSet::new(), import.span));
        for name in import.imported_names {
            entry.0.insert(name);
        }
    }

    by_module
        .into_iter()
        .map(|(module_name, (names, span))| ParsedRuntimeImport {
            module_name,
            imported_names: names.into_iter().collect(),
            span,
        })
        .collect()
}
