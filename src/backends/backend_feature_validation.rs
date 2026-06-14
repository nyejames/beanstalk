//! Pre-lowering validation for backend-specific HIR feature support.
//!
//! WHAT: rejects reachable HIR operations that are valid language semantics but unsupported by
//! a selected backend target.
//! WHY: backend lowerers should receive only features they can lower, and users should see a
//! structured source diagnostic instead of a backend-internal lowering error.

use crate::backends::external_package_validation::BackendTarget;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::reachability::{
    ReachableMapUse, ReachableMapUseKind, ReachableReactiveSinkKind, ReachableReactiveSinkUse,
    ReachableReactiveTemplateUse, collect_reachability_from_start,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Failure mode for backend feature validation.
///
/// WHAT: either a user-facing diagnostic for an unsupported reachable operation, or an
///       infrastructure error if reachability collection itself fails.
pub enum BackendFeatureValidationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

/// Validates HIR runtime features that are target-specific after frontend semantics are complete.
///
/// WHAT: hashmap construction/use and reactive runtime features are legal HIR, but only the JS
///       backend lowers them for Alpha. HTML-Wasm must reject reachable unsupported operations;
///       unused functions stay type checked but do not block the experimental Wasm build path.
/// WHY: fail early with a structured Rule error at the source location instead of a vague
///      backend-internal lowering failure.
pub fn validate_hir_backend_feature_support(
    hir: &HirModule,
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let reachability = collect_reachability_from_start(hir)
        .map_err(|error| BackendFeatureValidationError::Infrastructure(Box::new(error)))?;

    match target {
        BackendTarget::Wasm => {
            // Wasm does not yet lower hashmaps or any reactive runtime feature.
            validate_wasm_maps(&reachability.reachable_map_uses, target, string_table)?;
            validate_wasm_reactive_features(
                &reachability.reachable_reactive_templates,
                target,
                string_table,
            )?;
        }
        BackendTarget::Js => {
            // JS supports V1 top-level runtime fragment sinks, but not reactive template values
            // flowing into external/host calls such as `io(...)`.
            validate_js_reactive_sinks(
                hir,
                &reachability.reachable_reactive_sinks,
                target,
                string_table,
            )?;
        }
    }

    Ok(())
}

/// Reports the first reachable unsupported hashmap operation for the Wasm target.
///
/// WHAT: hashmap literals and operations are valid HIR, but Wasm lowering does not yet
/// support them.
/// WHY: reject early with a structured diagnostic at the source location instead of a
/// backend-internal lowering failure.
fn validate_wasm_maps(
    map_uses: &[ReachableMapUse],
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let Some(map_use) = map_uses.first() else {
        return Ok(());
    };

    let feature = match &map_use.kind {
        ReachableMapUseKind::Literal => "hashmap construction",
        ReachableMapUseKind::Operation(_) => "hashmap operation",
    };

    // Only the first reachable unsupported operation is reported. Unreachable helpers remain
    // valid typed HIR and do not block the build.
    let diagnostic = CompilerDiagnostic::unsupported_backend_feature(
        string_table.intern(target.as_str()),
        string_table.intern(feature),
        map_use.location.clone(),
    );

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        diagnostic,
    )))
}

/// Reports the first reachable reactive runtime feature for the Wasm target.
///
/// WHAT: reactive template values with runtime dependencies are valid HIR, but HTML-Wasm does
///       not yet have a reactive runtime design.
/// WHY: reject early with a structured diagnostic at the source location instead of a
///      backend-internal lowering failure. Unreachable helper functions containing reactive
///      templates remain valid typed HIR and do not block the build.
fn validate_wasm_reactive_features(
    reactive_templates: &[ReachableReactiveTemplateUse],
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let Some(reactive_template) = reactive_templates.first() else {
        return Ok(());
    };

    let diagnostic = CompilerDiagnostic::unsupported_backend_feature(
        string_table.intern(target.as_str()),
        string_table.intern("reactive template runtime"),
        reactive_template.location.clone(),
    );

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        diagnostic,
    )))
}

/// Reports the first reachable unsupported reactive sink for the JS target.
///
/// WHAT: JS supports V1 top-level runtime fragment sinks, but reactive template values with
///       runtime subscriptions passed to external/host calls such as `io(...)` are deferred.
///       Plain String parameters that merely *could* carry a reactive template are still allowed
///       at unsupported sinks until an actual reactive value flows there.
/// WHY: fail early with a structured diagnostic instead of silently snapshotting a reactive
///      template at an unsupported sink, while avoiding false positives from ordinary String
///      parameters.
fn validate_js_reactive_sinks(
    hir: &HirModule,
    reactive_sinks: &[ReachableReactiveSinkUse],
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let Some(rejected_sink) = reactive_sinks
        .iter()
        .filter(|sink| !matches!(sink.kind, ReachableReactiveSinkKind::RuntimeFragment))
        .find(|sink| sink_template_has_runtime_subscription(hir, sink))
    else {
        return Ok(());
    };

    let feature = feature_name_for_reactive_sink(rejected_sink);

    let diagnostic = CompilerDiagnostic::unsupported_backend_feature(
        string_table.intern(target.as_str()),
        string_table.intern(feature),
        rejected_sink.location.clone(),
    );

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        diagnostic,
    )))
}

/// Returns true when the template consumed by `sink` has at least one runtime subscription.
///
/// WHAT: a template with only template-value-parameter placeholders is not yet a live reactive
///       value; it needs an actual `$(source)` subscription to trigger the unsupported-sink rule.
fn sink_template_has_runtime_subscription(
    hir: &HirModule,
    sink: &ReachableReactiveSinkUse,
) -> bool {
    hir.side_table
        .reactive_templates()
        .find(|template| template.id == sink.template_id)
        .is_some_and(|template| !template.dependencies.is_empty())
}

fn feature_name_for_reactive_sink(sink: &ReachableReactiveSinkUse) -> &'static str {
    match &sink.kind {
        ReachableReactiveSinkKind::RuntimeFragment => "reactive runtime fragment",
        ReachableReactiveSinkKind::ExternalCallArgument { .. } => "reactive external-call sink",
    }
}
