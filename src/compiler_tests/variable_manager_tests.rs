//! Unit tests for VariableManager
//!
//! Tests for variable declarations, references, assignments, and scope tracking.

use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::nodes::{HirExprKind, HirPlace};
use crate::compiler::hir::variable_manager::VariableManager;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

#[test]
fn test_new_variable_manager() {
    let manager = VariableManager::new();
    assert_eq!(manager.current_scope_level(), 0);
}

#[test]
fn test_enter_exit_scope() {
    let mut manager = VariableManager::new();

    assert_eq!(manager.current_scope_level(), 0);

    manager.enter_scope();
    assert_eq!(manager.current_scope_level(), 1);

    manager.enter_scope();
    assert_eq!(manager.current_scope_level(), 2);

    manager.exit_scope();
    assert_eq!(manager.current_scope_level(), 1);

    manager.exit_scope();
    assert_eq!(manager.current_scope_level(), 0);
}

#[test]
fn test_declare_variable() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    let var_name = ctx.string_table.intern("test_var");
    let result = manager.declare_variable(
        var_name,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );

    assert!(result.is_ok());
    assert!(manager.variable_exists(var_name));
    assert!(!manager.is_variable_mutable(var_name));
    assert!(!manager.is_ownership_capable(var_name)); // Int is not ownership capable
}

#[test]
fn test_declare_mutable_variable() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    let var_name = ctx.string_table.intern("mutable_var");
    let result = manager.declare_variable(
        var_name,
        DataType::Int,
        true,
        TextLocation::default(),
        &mut ctx,
    );

    assert!(result.is_ok());
    assert!(manager.is_variable_mutable(var_name));
}

#[test]
fn test_ownership_capable_types() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    // Struct should be ownership capable
    let struct_var = ctx.string_table.intern("struct_var");
    let _ = manager.declare_variable(
        struct_var,
        DataType::Struct(vec![], Ownership::default()),
        false,
        TextLocation::default(),
        &mut ctx,
    );
    assert!(manager.is_ownership_capable(struct_var));

    // Int should not be ownership capable
    let int_var = ctx.string_table.intern("int_var");
    let _ = manager.declare_variable(
        int_var,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );
    assert!(!manager.is_ownership_capable(int_var));
}

#[test]
fn test_variable_scope_cleanup() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    // Declare variable in scope 0
    let outer_var = ctx.string_table.intern("outer");
    let _ = manager.declare_variable(
        outer_var,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );

    // Enter new scope and declare variable
    manager.enter_scope();
    let inner_var = ctx.string_table.intern("inner");
    let _ = manager.declare_variable(
        inner_var,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );

    assert!(manager.variable_exists(outer_var));
    assert!(manager.variable_exists(inner_var));

    // Exit scope - inner variable should be cleaned up
    let exited = manager.exit_scope();
    assert!(exited.contains(&inner_var));
    assert!(manager.variable_exists(outer_var));
    assert!(!manager.variable_exists(inner_var));
}

#[test]
fn test_reference_variable() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    let var_name = ctx.string_table.intern("ref_var");
    let _ = manager.declare_variable(
        var_name,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );

    let result = manager.reference_variable(var_name, TextLocation::default(), &mut ctx);
    assert!(result.is_ok());

    let expr = result.unwrap();
    assert!(matches!(expr.kind, HirExprKind::Load(HirPlace::Var(_))));
    assert!(matches!(expr.data_type, DataType::Int));
}

#[test]
fn test_reference_undeclared_variable() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    let var_name = ctx.string_table.intern("undeclared");
    let result = manager.reference_variable(var_name, TextLocation::default(), &mut ctx);
    assert!(result.is_err());
}

#[test]
fn test_mark_potential_move() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    // Ownership-capable variable
    let struct_var = ctx.string_table.intern("struct_var");
    let _ = manager.declare_variable(
        struct_var,
        DataType::Struct(vec![], Ownership::default()),
        false,
        TextLocation::default(),
        &mut ctx,
    );

    let result = manager.mark_potential_move(struct_var, TextLocation::default(), &mut ctx);
    assert!(result.is_ok());

    let expr = result.unwrap();
    assert!(matches!(expr.kind, HirExprKind::Move(_)));
}

#[test]
fn test_mark_potential_move_non_ownership_capable() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut manager = VariableManager::new();

    // Non-ownership-capable variable (Int)
    let int_var = ctx.string_table.intern("int_var");
    let _ = manager.declare_variable(
        int_var,
        DataType::Int,
        false,
        TextLocation::default(),
        &mut ctx,
    );

    let result = manager.mark_potential_move(int_var, TextLocation::default(), &mut ctx);
    assert!(result.is_ok());

    let expr = result.unwrap();
    // Should be a Load, not a Move, for non-ownership-capable types
    assert!(matches!(expr.kind, HirExprKind::Load(_)));
}
