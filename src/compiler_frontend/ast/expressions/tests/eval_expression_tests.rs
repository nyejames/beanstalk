use super::*;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::{
    CompileTimePath, CompileTimePathBase, CompileTimePathKind, CompileTimePaths,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

fn template_ast_node(template: Template) -> AstNode {
    AstNode {
        kind: NodeKind::Rvalue(Expression::template(template, Ownership::ImmutableOwned)),
        location: SourceLocation::default(),
        scope: InternedPath::new(),
    }
}

#[test]
fn concat_template_preserves_full_style_state_from_last_template() {
    let mut left = Template::create_default(vec![]);
    left.style.id = "markdown";
    left.style.skip_parent_child_wrappers = true;
    left.explicit_style.id = "markdown";
    left.explicit_style.skip_parent_child_wrappers = true;

    let mut right = Template::create_default(vec![]);
    right.style.id = "css";
    right.style.skip_parent_child_wrappers = true;
    right.style.child_templates = vec![Template::create_default(vec![])];
    right.explicit_style.id = "css";
    right.explicit_style.skip_parent_child_wrappers = true;
    right.explicit_style.child_templates = vec![Template::create_default(vec![])];

    let mut nodes = vec![template_ast_node(left), template_ast_node(right.clone())];
    let concatenated = concat_template(&mut nodes, Ownership::ImmutableOwned)
        .expect("template concatenation should succeed");

    let ExpressionKind::Template(result) = concatenated.kind else {
        panic!("expected concatenated template expression");
    };

    assert_eq!(result.style.id, right.style.id);
    assert_eq!(
        result.style.skip_parent_child_wrappers,
        right.style.skip_parent_child_wrappers
    );
    assert_eq!(
        result.style.child_templates.len(),
        right.style.child_templates.len()
    );

    assert_eq!(result.explicit_style.id, right.explicit_style.id);
    assert_eq!(
        result.explicit_style.skip_parent_child_wrappers,
        right.explicit_style.skip_parent_child_wrappers
    );
    assert_eq!(
        result.explicit_style.child_templates.len(),
        right.explicit_style.child_templates.len()
    );
}

#[test]
fn coerce_to_string_records_rendered_path_usages_for_path_values() {
    let mut string_table = StringTable::new();
    let source_scope = InternedPath::from_single_str("#page.bst", &mut string_table);
    let asset_path = InternedPath::from_single_str("assets", &mut string_table)
        .join_str("logo.png", &mut string_table);
    let compile_time_paths = CompileTimePaths {
        paths: vec![CompileTimePath {
            source_path: asset_path.clone(),
            filesystem_path: std::env::temp_dir().join("beanstalk_eval_expression_logo.png"),
            public_path: asset_path.clone(),
            base: CompileTimePathBase::ProjectRootFolder,
            kind: CompileTimePathKind::File,
        }],
    };
    let context = ScopeContext::new(
        ContextKind::Template,
        source_scope.clone(),
        &[],
        HostRegistry::new(),
        vec![],
    )
    .with_source_file_scope(source_scope.clone())
    .with_path_format_config(PathStringFormatConfig {
        origin: String::from("/beanstalk"),
        ..PathStringFormatConfig::default()
    });

    let nodes = vec![
        AstNode {
            kind: NodeKind::Rvalue(Expression::path(
                compile_time_paths,
                SourceLocation::default(),
            )),
            location: SourceLocation::default(),
            scope: source_scope.clone(),
        },
        AstNode {
            kind: NodeKind::Operator(Operator::Add),
            location: SourceLocation::default(),
            scope: source_scope.clone(),
        },
        AstNode {
            kind: NodeKind::Rvalue(Expression::string_slice(
                string_table.get_or_intern(String::from("?v=1")),
                SourceLocation::default(),
                Ownership::ImmutableOwned,
            )),
            location: SourceLocation::default(),
            scope: source_scope,
        },
    ];

    let mut current_type = DataType::CoerceToString;
    let result = evaluate_expression(
        &context,
        nodes,
        &mut current_type,
        &Ownership::ImmutableOwned,
        &mut string_table,
    )
    .expect("coercion should succeed");

    let ExpressionKind::StringSlice(rendered_id) = result.kind else {
        panic!("expected string result");
    };
    assert_eq!(
        string_table.resolve(rendered_id),
        "/beanstalk/assets/logo.png?v=1"
    );

    let recorded = context.take_rendered_path_usages();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].source_path, asset_path);
    assert_eq!(recorded[0].base, CompileTimePathBase::ProjectRootFolder);
}
