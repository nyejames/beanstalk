//! Unit tests for `ScopeContext` construction invariants.
//!
//! WHAT: validates builder-pattern behaviour, context-kind transitions, and
//! visibility-gate synchronisation for the most commonly misused paths.
//! WHY: these properties are refactor seams — subtle mismatches (e.g. dropping
//! `expected_error_type` on context clone) silently corrupt later passes.

use super::environment::TopLevelDeclarationTable;
use super::scope_context::{ContextKind, ScopeContext};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashSet;
use std::rc::Rc;

fn empty_scope(string_table: &mut StringTable) -> InternedPath {
    let mut path = InternedPath::new();
    path.push_str("test_scope", string_table);
    path
}

// ---------------------------------------------------------------------------
// new() invariants
// ---------------------------------------------------------------------------

#[test]
fn scope_context_new_leaves_no_visibility_gate() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let context = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    assert!(
        context.visible_declaration_ids.is_none(),
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
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
    use crate::compiler_frontend::value_mode::ValueMode;

    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let mut context = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );

    // Install an empty visibility gate.
    context = context.with_visible_declarations(FxHashSet::default());

    let variable_path = InternedPath::from_components(vec![string_table.intern("my_var")]);
    let declaration = Declaration {
        id: variable_path.to_owned(),
        value: Expression::new(
            ExpressionKind::NoValue,
            SourceLocation::default(),
            DataType::Int,
            ValueMode::ImmutableOwned,
        ),
    };
    context.add_var(declaration);

    assert!(
        context
            .visible_declaration_ids
            .as_ref()
            .unwrap()
            .contains(&variable_path),
        "add_var must insert the new variable into the visibility gate"
    );
    assert_eq!(
        context.local_declarations.len(),
        1,
        "add_var must append the declaration to the local declaration layer"
    );
}

// ---------------------------------------------------------------------------
// new_template_parsing_context
// ---------------------------------------------------------------------------

#[test]
fn new_template_parsing_context_preserves_constant_kind() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let context = ScopeContext::new(
        ContextKind::Constant,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    let template = context.new_template_parsing_context();
    assert_eq!(
        template.kind,
        ContextKind::Constant,
        "constant contexts must stay constant when creating a template child"
    );
}

#[test]
fn new_template_parsing_context_converts_function_kind_to_template() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let context = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    let template = context.new_template_parsing_context();
    assert_eq!(
        template.kind,
        ContextKind::Template,
        "non-constant contexts must become Template kind in a template child"
    );
}

#[test]
fn new_template_parsing_context_propagates_expected_error_type() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let mut context = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    context.expected_error_type = Some(DataType::StringSlice);

    let template = context.new_template_parsing_context();
    assert_eq!(
        template.expected_error_type,
        Some(DataType::StringSlice),
        "new_template_parsing_context must propagate expected_error_type"
    );
}

// ---------------------------------------------------------------------------
// new_child_control_flow — loop depth
// ---------------------------------------------------------------------------

#[test]
fn new_child_control_flow_increments_loop_depth_for_loop_kind() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let context = ScopeContext::new(
        ContextKind::Function,
        scope,
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    assert_eq!(context.loop_depth, 0);

    let loop_context = context.new_child_control_flow(ContextKind::Loop, &mut string_table);
    assert_eq!(
        loop_context.loop_depth, 1,
        "entering a Loop scope must increment loop_depth"
    );

    let branch_context =
        loop_context.new_child_control_flow(ContextKind::Branch, &mut string_table);
    assert_eq!(
        branch_context.loop_depth, 1,
        "entering a Branch scope must not change loop_depth"
    );
}

// ---------------------------------------------------------------------------
// new_constant — inherits parent visibility gate
// ---------------------------------------------------------------------------

#[test]
fn new_constant_inherits_parent_visibility_gate() {
    let mut string_table = StringTable::new();
    let scope = empty_scope(&mut string_table);
    let mut context = ScopeContext::new(
        ContextKind::Function,
        scope.to_owned(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );

    let mut visibility_gate = FxHashSet::default();
    let gated_path = InternedPath::from_components(vec![string_table.intern("gated")]);
    visibility_gate.insert(gated_path.to_owned());
    context = context.with_visible_declarations(visibility_gate);

    let constant_scope = InternedPath::from_components(vec![string_table.intern("const_scope")]);
    let constant_context = ScopeContext::new_constant(constant_scope, &context);

    assert!(
        constant_context
            .visible_declaration_ids
            .as_ref()
            .unwrap()
            .contains(&gated_path),
        "new_constant must inherit the parent's visibility gate"
    );
}
