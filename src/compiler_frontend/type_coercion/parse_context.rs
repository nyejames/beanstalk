//! Parse-time context helpers for the Beanstalk compiler frontend.
//!
//! WHAT: determines what expected type should be threaded into expression
//! parsing at assignment-like sites (declarations, mutations, struct fields,
//! collection items).
//! WHY: most types should be resolved strictly by the expression parser with
//! no expected-type hint (`Inferred`), so that `eval_expression` operates in
//! `Exact` context and callers own their own post-parse coercion. However,
//! `none` is a parse-context-sensitive literal — it can only resolve its inner
//! type when the surrounding context supplies an `Option(_)` expected type.
//! Without that hint the parser must reject the literal immediately, because
//! there is no post-parse coercion that can "invent" a missing inner type.
//!
//! ## Rule
//!
//! - `Option(_)` targets: pass the full option type through so `none` can
//!   extract its inner type at parse time.
//! - All other targets: pass `Inferred` so the expression resolves its own
//!   natural type and the call site validates/coerces after the fact.

use crate::compiler_frontend::datatypes::DataType;

/// Returns the expected type that should be passed into expression parsing
/// for a given target type.
///
/// WHAT: the single authoritative rule for what parse-time context is
/// preserved at assignment-like sites.
/// WHY: only `Option(_)` needs parse-time context today (for `none` literals).
/// All other targets use `Inferred` to keep the expression parser strict and
/// push coercion ownership to the call site.
pub(crate) fn parse_expectation_for_target_type(target: &DataType) -> DataType {
    match target {
        DataType::Option(_) => target.clone(),
        _ => DataType::Inferred,
    }
}
