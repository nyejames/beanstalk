//! Parse-time context helpers for the Beanstalk compiler frontend.
//!
//! WHAT: determines what expected type should be threaded into expression
//! parsing at assignment-like sites (declarations, mutations, struct fields,
//! collection items).
//! WHY: most types should be resolved strictly by the expression parser with
//! no expected-type hint (`Inferred`), so that `eval_expression` operates in
//! `Exact` context and callers own their own post-parse coercion. However, some
//! literals are parse-context-sensitive: `none` needs an `Option(_)` target, and
//! empty collection literals need an explicit `Collection(T)` target. Without
//! those hints the parser must reject the literal immediately, because there is
//! no post-parse coercion that can "invent" a missing inner type.
//!
//! ## Rule
//!
//! - `Option(_)` targets: pass the full option type through so `none` can
//!   extract its inner type at parse time.
//! - Explicit `Collection(T)` targets: pass the collection type through so
//!   empty collection literals can resolve their element type.
//! - All other targets: pass `Inferred` so the expression resolves its own
//!   natural type and the call site validates/coerces after the fact.

use crate::compiler_frontend::datatypes::DataType;

/// Returns the expected type that should be passed into expression parsing
/// for a given target type.
///
/// WHAT: the single authoritative rule for what parse-time context is
/// preserved at assignment-like sites.
/// WHY: only parse-context-sensitive literals keep target context. All other
/// targets use `Inferred` to keep the expression parser strict and push
/// coercion ownership to the call site.
pub(crate) fn parse_expectation_for_target_type(target: &DataType) -> DataType {
    match target {
        DataType::Option(_) => target.clone(),
        DataType::Collection(inner) if !matches!(inner.as_ref(), DataType::Inferred) => {
            target.clone()
        }
        _ => DataType::Inferred,
    }
}
