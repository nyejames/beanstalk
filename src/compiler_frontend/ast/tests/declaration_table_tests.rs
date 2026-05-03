//! Stable declaration table regression tests.
//!
//! WHAT: checks the AST environment-owned top-level declaration table independent of parser
//! setup.
//! WHY: phase 3 relies on updates preserving placeholder slots so later lookups observe resolved
//! metadata without rebuilding declaration snapshots.

use super::environment::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

#[test]
fn updates_existing_declaration_slot_without_reordering() {
    let mut string_table = StringTable::new();
    let first_path = InternedPath::from_single_str("first", &mut string_table);
    let second_path = InternedPath::from_single_str("second", &mut string_table);

    let mut table = TopLevelDeclarationTable::new(vec![
        declaration(&first_path, DataType::Inferred),
        declaration(&second_path, DataType::Bool),
    ]);

    table
        .replace_by_path(declaration(&first_path, DataType::StringSlice))
        .expect("existing declaration path should update in place");

    let declarations = table.iter().collect::<Vec<_>>();
    assert_eq!(declarations[0].id, first_path);
    assert_eq!(declarations[0].value.data_type, DataType::StringSlice);
    assert_eq!(declarations[1].id, second_path);
    assert_eq!(declarations[1].value.data_type, DataType::Bool);

    let first_name = first_path.name().expect("test path should have a name");
    let by_name = table
        .get_visible_resolved_by_name(first_name, None)
        .expect("name lookup should see updated declaration");
    assert_eq!(by_name.value.data_type, DataType::StringSlice);
}

fn declaration(path: &InternedPath, data_type: DataType) -> Declaration {
    Declaration {
        id: path.to_owned(),
        value: Expression::no_value(
            SourceLocation::default(),
            data_type,
            ValueMode::ImmutableOwned,
        ),
    }
}
