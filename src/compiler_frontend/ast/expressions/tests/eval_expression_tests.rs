use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::CssDirectiveMode;
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
    left.style.formatter_precedence = 1;
    left.style.override_precedence = 2;
    left.style.clear_inherited = true;
    left.explicit_style.id = "markdown";
    left.explicit_style.override_precedence = 2;
    left.explicit_style.clear_inherited = true;

    let mut right = Template::create_default(vec![]);
    right.style.id = "css";
    right.style.css_mode = Some(CssDirectiveMode::Inline);
    right.style.formatter_precedence = 11;
    right.style.override_precedence = 12;
    right.style.clear_inherited = true;
    right.style.child_templates = vec![Template::create_default(vec![])];
    right.explicit_style.id = "css";
    right.explicit_style.css_mode = Some(CssDirectiveMode::Inline);
    right.explicit_style.formatter_precedence = 21;
    right.explicit_style.override_precedence = 22;
    right.explicit_style.clear_inherited = true;
    right.explicit_style.child_templates = vec![Template::create_default(vec![])];

    let mut nodes = vec![template_ast_node(left), template_ast_node(right.clone())];
    let concatenated = concat_template(&mut nodes, Ownership::ImmutableOwned)
        .expect("template concatenation should succeed");

    let ExpressionKind::Template(result) = concatenated.kind else {
        panic!("expected concatenated template expression");
    };

    assert_eq!(result.style.id, right.style.id);
    assert_eq!(result.style.css_mode, right.style.css_mode);
    assert_eq!(
        result.style.formatter_precedence,
        right.style.formatter_precedence
    );
    assert_eq!(
        result.style.override_precedence,
        right.style.override_precedence
    );
    assert_eq!(result.style.clear_inherited, right.style.clear_inherited);
    assert_eq!(
        result.style.child_templates.len(),
        right.style.child_templates.len()
    );

    assert_eq!(result.explicit_style.id, right.explicit_style.id);
    assert_eq!(
        result.explicit_style.css_mode,
        right.explicit_style.css_mode
    );
    assert_eq!(
        result.explicit_style.formatter_precedence,
        right.explicit_style.formatter_precedence
    );
    assert_eq!(
        result.explicit_style.override_precedence,
        right.explicit_style.override_precedence
    );
    assert_eq!(
        result.explicit_style.clear_inherited,
        right.explicit_style.clear_inherited
    );
    assert_eq!(
        result.explicit_style.child_templates.len(),
        right.explicit_style.child_templates.len()
    );
}
