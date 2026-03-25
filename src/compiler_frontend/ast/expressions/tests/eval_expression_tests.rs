use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

fn template_ast_node(template: Template) -> AstNode {
    AstNode {
        kind: NodeKind::Rvalue(Expression::template(template, Ownership::ImmutableOwned)),
        location: TextLocation::default(),
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
