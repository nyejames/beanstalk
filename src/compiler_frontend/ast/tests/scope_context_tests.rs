//! Unit tests for `ScopeContext` construction invariants.
//!
//! WHAT: validates builder-pattern behaviour, context-kind transitions, and
//! visibility-gate synchronisation for the most commonly misused paths.
//! WHY: these properties are refactor seams — subtle mismatches (e.g. dropping
//! `expected_error_type` on context clone) silently corrupt later passes.

use super::*;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::rc::Rc;

fn empty_scope(string_table: &mut StringTable) -> InternedPath {
    let mut p = InternedPath::new();
    p.push_str("test_scope", string_table);
    p
}

// ---------------------------------------------------------------------------
// new() invariants
// ---------------------------------------------------------------------------

#[test]
fn scope_context_new_leaves_no_visibility_gate() {
    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let ctx = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );
    assert!(
        ctx.visible_declaration_ids.is_none(),
        "ScopeContext::new must not install a visibility gate by default"
    );
}

// ---------------------------------------------------------------------------
// add_var — gate synchronisation
// ---------------------------------------------------------------------------

#[test]
fn add_var_extends_visibility_gate_when_gate_is_set() {
    use crate::compiler_frontend::ast::ast_nodes::Declaration;
    use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
    use crate::compiler_frontend::datatypes::Ownership;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
    use rustc_hash::FxHashSet;

    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let mut ctx = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );

    // Install an empty visibility gate.
    ctx = ctx.with_visible_declarations(FxHashSet::default());

    let var_path = InternedPath::from_components(vec![st.intern("my_var")]);
    let decl = Declaration {
        id: var_path.to_owned(),
        value: Expression::new(
            ExpressionKind::NoValue,
            SourceLocation::default(),
            DataType::Int,
            Ownership::ImmutableOwned,
        ),
    };
    ctx.add_var(decl);

    assert!(
        ctx.visible_declaration_ids
            .as_ref()
            .unwrap()
            .contains(&var_path),
        "add_var must insert the new variable into the visibility gate"
    );
    assert_eq!(
        ctx.local_declarations.len(),
        1,
        "add_var must append the declaration to the local declaration layer"
    );
}

// ---------------------------------------------------------------------------
// new_template_parsing_context
// ---------------------------------------------------------------------------

#[test]
fn new_template_parsing_context_preserves_constant_kind() {
    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let ctx = ScopeContext::new(
        ContextKind::Constant,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );
    let tpl = ctx.new_template_parsing_context();
    assert_eq!(
        tpl.kind,
        ContextKind::Constant,
        "constant contexts must stay constant when creating a template child"
    );
}

#[test]
fn new_template_parsing_context_converts_function_kind_to_template() {
    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let ctx = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );
    let tpl = ctx.new_template_parsing_context();
    assert_eq!(
        tpl.kind,
        ContextKind::Template,
        "non-constant contexts must become Template kind in a template child"
    );
}

#[test]
fn new_template_parsing_context_propagates_expected_error_type() {
    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let mut ctx = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );
    ctx.expected_error_type = Some(DataType::StringSlice);

    let tpl = ctx.new_template_parsing_context();
    assert_eq!(
        tpl.expected_error_type,
        Some(DataType::StringSlice),
        "new_template_parsing_context must propagate expected_error_type"
    );
}

// ---------------------------------------------------------------------------
// new_child_control_flow — loop depth
// ---------------------------------------------------------------------------

#[test]
fn new_child_control_flow_increments_loop_depth_for_loop_kind() {
    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let ctx = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );
    assert_eq!(ctx.loop_depth, 0);

    let loop_ctx = ctx.new_child_control_flow(ContextKind::Loop, &mut st);
    assert_eq!(
        loop_ctx.loop_depth, 1,
        "entering a Loop scope must increment loop_depth"
    );

    let branch_ctx = loop_ctx.new_child_control_flow(ContextKind::Branch, &mut st);
    assert_eq!(
        branch_ctx.loop_depth, 1,
        "entering a Branch scope must not change loop_depth"
    );
}

// ---------------------------------------------------------------------------
// new_constant — inherits parent visibility gate
// ---------------------------------------------------------------------------

#[test]
fn new_constant_inherits_parent_visibility_gate() {
    use rustc_hash::FxHashSet;

    let mut st = StringTable::new();
    let scope = empty_scope(&mut st);
    let mut ctx = ScopeContext::new(
        ContextKind::Function,
        scope.to_owned(),
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );

    let mut gate = FxHashSet::default();
    let gated_path = InternedPath::from_components(vec![st.intern("gated")]);
    gate.insert(gated_path.to_owned());
    ctx = ctx.with_visible_declarations(gate);

    let const_scope = InternedPath::from_components(vec![st.intern("const_scope")]);
    let const_ctx = ScopeContext::new_constant(const_scope, &ctx);

    assert!(
        const_ctx
            .visible_declaration_ids
            .as_ref()
            .unwrap()
            .contains(&gated_path),
        "new_constant must inherit the parent's visibility gate"
    );
}
