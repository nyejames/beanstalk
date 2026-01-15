//! Unit tests for Drop Point Inserter
//!
//! These tests verify the basic functionality of the DropPointInserter component.

#[cfg(test)]
mod drop_point_inserter_unit_tests {
    use crate::compiler::hir::build_hir::HirBuilderContext;
    use crate::compiler::hir::memory_management::drop_point_inserter::DropPointInserter;
    use crate::compiler::hir::nodes::{HirKind, HirPlace, HirStmt};
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;

    #[test]
    fn test_drop_inserter_creation() {
        let mut string_table = StringTable::new();
        let var_name = string_table.intern("test_var");
        let ctx = HirBuilderContext::new(&mut string_table);
        let inserter = DropPointInserter::new();
        
        assert!(!inserter.is_ownership_capable(&HirPlace::Var(var_name), &ctx));
    }

    #[test]
    fn test_scope_exit_drops() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut inserter = DropPointInserter::new();

        let var_name = ctx.string_table.intern("test_var");
        ctx.mark_potentially_owned(var_name);
        ctx.add_drop_candidate(var_name, TextLocation::default());

        let drops = inserter.insert_scope_exit_drops(&[var_name], &mut ctx);
        assert_eq!(drops.len(), 1);

        // Verify the drop node is correct
        if let HirKind::Stmt(HirStmt::PossibleDrop(HirPlace::Var(name))) = &drops[0].kind {
            assert_eq!(*name, var_name);
        } else {
            panic!("Expected PossibleDrop statement");
        }
    }

    #[test]
    fn test_return_drops() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut inserter = DropPointInserter::new();

        let var1 = ctx.string_table.intern("var1");
        let var2 = ctx.string_table.intern("var2");

        ctx.mark_potentially_owned(var1);
        ctx.mark_potentially_owned(var2);
        ctx.add_drop_candidate(var1, TextLocation::default());
        ctx.add_drop_candidate(var2, TextLocation::default());

        let drops = inserter.insert_return_drops(&[var1, var2], &mut ctx);
        assert_eq!(drops.len(), 2);
    }

    #[test]
    fn test_ownership_capability_check() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let inserter = DropPointInserter::new();

        let var_name = ctx.string_table.intern("owned_var");
        ctx.mark_potentially_owned(var_name);

        assert!(inserter.is_ownership_capable(&HirPlace::Var(var_name), &ctx));

        let non_owned = ctx.string_table.intern("borrowed_var");
        ctx.mark_definitely_borrowed(non_owned);

        assert!(!inserter.is_ownership_capable(&HirPlace::Var(non_owned), &ctx));
    }

    #[test]
    fn test_field_access_ownership_capability() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let inserter = DropPointInserter::new();

        let base_var = ctx.string_table.intern("struct_var");
        let field_name = ctx.string_table.intern("field");

        ctx.mark_potentially_owned(base_var);

        let field_place = HirPlace::Field {
            base: Box::new(HirPlace::Var(base_var)),
            field: field_name,
        };

        // Field access inherits ownership capability from base
        assert!(inserter.is_ownership_capable(&field_place, &ctx));
    }
}
