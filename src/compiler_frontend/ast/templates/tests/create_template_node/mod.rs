use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateSegment};
use crate::compiler_frontend::ast::templates::template_render_plan::RenderPiece;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::WarningKind;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::{StyleDirectiveRegistry, StyleDirectiveSpec};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{
    CharPosition, FileTokens, SourceLocation, TemplateBodyMode, Token, TokenKind,
};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::html_project::style_directives::html_project_style_directives;
use std::rc::Rc;

fn frontend_test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::built_ins()
}

fn html_project_test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::merged(&html_project_style_directives())
        .expect("html project style directives should merge with core directives")
}

fn token(kind: TokenKind, line: i32) -> Token {
    Token::new(
        kind,
        SourceLocation {
            scope: InternedPath::new(),
            start_pos: CharPosition {
                line_number: line,
                char_column: 0,
            },
            end_pos: CharPosition {
                line_number: line,
                char_column: 120, // Arbitrary number
            },
        },
    )
}

fn template_tokens_from_source(source: &str, string_table: &mut StringTable) -> FileTokens {
    let style_directives = frontend_test_style_directives();
    template_tokens_from_source_with_style_directives(source, &style_directives, string_table)
}

fn template_tokens_from_source_with_style_directives(
    source: &str,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> FileTokens {
    let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
    let mut tokens = tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        style_directives,
        string_table,
        None,
    )
    .expect("tokenization should succeed");

    tokens.index = tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::TemplateHead))
        .expect("expected a template opener");

    tokens
}

fn template_tokens_from_source_with_directives(
    source: &str,
    directives: &[StyleDirectiveSpec],
    string_table: &mut StringTable,
) -> FileTokens {
    let registry = StyleDirectiveRegistry::merged(directives)
        .expect("test style directives should merge with core directives");
    let mut tokens =
        template_tokens_from_source_with_style_directives(source, &registry, string_table);

    tokens.index = tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::TemplateHead))
        .expect("expected a template opener");

    tokens
}

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(cwd.clone(), cwd, &[]).expect("test path resolver should be valid")
}

fn with_test_path_context(
    context: ScopeContext,
    source_scope: &InternedPath,
    style_directives: &StyleDirectiveRegistry,
) -> ScopeContext {
    context
        .with_style_directives(style_directives)
        .with_project_path_resolver(Some(test_project_path_resolver()))
        .with_source_file_scope(source_scope.to_owned())
        .with_path_format_config(PathStringFormatConfig::default())
}

fn new_constant_context(scope: InternedPath) -> ScopeContext {
    let style_directives = frontend_test_style_directives();
    new_constant_context_with_style_directives(scope, &style_directives)
}

fn new_constant_context_with_style_directives(
    scope: InternedPath,
    style_directives: &StyleDirectiveRegistry,
) -> ScopeContext {
    let parent = with_test_path_context(
        ScopeContext::new(
            ContextKind::Constant,
            scope.to_owned(),
            Rc::new(TopLevelDeclarationIndex::new(vec![])),
            ExternalPackageRegistry::default(),
            vec![],
        ),
        &scope,
        style_directives,
    );
    ScopeContext::new_constant(scope, &parent)
}

fn fold_template_in_context(
    template: &Template,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> StringId {
    let mut fold_context = context
        .new_template_fold_context(string_table, "template tests fold")
        .expect("test context should include fold dependencies");
    template
        .fold_into_stringid(&mut fold_context)
        .expect("template should fold")
}

fn runtime_template_context(scope: &InternedPath, string_table: &mut StringTable) -> ScopeContext {
    let style_directives = frontend_test_style_directives();
    runtime_template_context_with_style_directives(scope, &style_directives, string_table)
}

fn runtime_template_context_with_style_directives(
    scope: &InternedPath,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> ScopeContext {
    let value_name = string_table.intern("value");
    let declaration = Declaration {
        id: scope.append(value_name),
        value: Expression::string_slice(
            string_table.intern("dynamic"),
            SourceLocation {
                scope: InternedPath::new(),
                start_pos: CharPosition {
                    line_number: 1,
                    char_column: 0,
                },
                end_pos: CharPosition {
                    line_number: 1,
                    char_column: 120, // Arbitrary number
                },
            },
            ValueMode::ImmutableOwned,
        ),
    };

    with_test_path_context(
        ScopeContext::new(
            ContextKind::Template,
            scope.to_owned(),
            Rc::new(TopLevelDeclarationIndex::new(vec![declaration])),
            ExternalPackageRegistry::default(),
            vec![],
        ),
        scope,
        style_directives,
    )
}

fn constant_template_context(scope: &InternedPath, declarations: &[Declaration]) -> ScopeContext {
    let style_directives = frontend_test_style_directives();
    constant_template_context_with_style_directives(scope, declarations, &style_directives)
}

fn constant_template_context_with_style_directives(
    scope: &InternedPath,
    declarations: &[Declaration],
    style_directives: &StyleDirectiveRegistry,
) -> ScopeContext {
    with_test_path_context(
        ScopeContext::new(
            ContextKind::Constant,
            scope.to_owned(),
            Rc::new(TopLevelDeclarationIndex::new(declarations.to_vec())),
            ExternalPackageRegistry::default(),
            vec![],
        ),
        scope,
        style_directives,
    )
}

fn docs_style_wrapper_declarations(string_table: &mut StringTable) -> Vec<Declaration> {
    let wrapper_scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);

    let mut table_tokens = template_tokens_from_source(
        "[:
      <table style=\"[$slot(\"style\")]\">
        [$slot]
      </table>
    ]",
        string_table,
    );
    let table_context = new_constant_context(table_tokens.src_path.to_owned());
    let table = Template::new(&mut table_tokens, &table_context, vec![], string_table)
        .expect("table wrapper should parse");

    let mut row_tokens = template_tokens_from_source(
        "[:
    <tr>[$fresh, $children([:<td>[$slot]</td>]):[$slot]]</tr>
]",
        string_table,
    );
    let row_context = new_constant_context(row_tokens.src_path.to_owned());
    let row = Template::new(&mut row_tokens, &row_context, vec![], string_table)
        .expect("row wrapper should parse");

    let mut header_row_tokens = template_tokens_from_source(
        "[:
    <tr>
        [$fresh, $children([:
            <th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">[$slot]</th>
        ]):[$slot]]
    </tr>
]",
        string_table,
    );
    let header_row_context = new_constant_context(header_row_tokens.src_path.to_owned());
    let header_row = Template::new(
        &mut header_row_tokens,
        &header_row_context,
        vec![],
        string_table,
    )
    .expect("header row wrapper should parse");

    vec![
        Declaration {
            id: wrapper_scope.append(string_table.intern("table")),
            value: Expression::template(table, ValueMode::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("row")),
            value: Expression::template(row, ValueMode::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("header_row")),
            value: Expression::template(header_row, ValueMode::ImmutableOwned),
        },
    ]
}

fn folded_template_output(source: &str) -> String {
    let style_directives = frontend_test_style_directives();
    folded_template_output_with_style_directives(source, &style_directives)
}

fn folded_template_output_with_style_directives(
    source: &str,
    style_directives: &StyleDirectiveRegistry,
) -> String {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        source,
        style_directives,
        &mut string_table,
    );
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        style_directives,
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    string_table.resolve(folded).to_owned()
}

fn template_parse_error(source: &str) -> String {
    let style_directives = frontend_test_style_directives();
    template_parse_error_with_style_directives(source, &style_directives)
}

fn template_parse_compiler_error_with_style_directives(
    source: &str,
    style_directives: &StyleDirectiveRegistry,
) -> CompilerError {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut token_stream = match tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        style_directives,
        &mut string_table,
        None,
    ) {
        Ok(tokens) => tokens,
        Err(error) => return error,
    };
    token_stream.index = token_stream
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::TemplateHead))
        .expect("expected a template opener");
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        style_directives,
    );

    Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template should fail to parse")
}

fn template_parse_error_with_style_directives(
    source: &str,
    style_directives: &StyleDirectiveRegistry,
) -> String {
    template_parse_compiler_error_with_style_directives(source, style_directives).msg
}

fn template_warnings_with_style_directives(
    source: &str,
    runtime_context: bool,
    style_directives: &StyleDirectiveRegistry,
) -> Vec<crate::compiler_frontend::compiler_warnings::CompilerWarning> {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        source,
        style_directives,
        &mut string_table,
    );
    let context = if runtime_context {
        runtime_template_context_with_style_directives(
            &token_stream.src_path,
            style_directives,
            &mut string_table,
        )
    } else {
        new_constant_context_with_style_directives(
            token_stream.src_path.to_owned(),
            style_directives,
        )
    };

    let _ = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse for warning checks");
    context.take_emitted_warnings()
}

fn template_segments(template: &Template) -> Vec<&TemplateSegment> {
    template
        .content
        .atoms
        .iter()
        .filter_map(|atom| match atom {
            TemplateAtom::Content(segment) => Some(segment),
            TemplateAtom::Slot(_) => None,
        })
        .collect()
}

/// Collects the resolved text strings from all body-origin text pieces in the
/// template's render plan. This is the correct way to inspect formatted body
/// content after parsing, since formatting is applied to the render plan rather
/// than rewritten back into `template.content`.
fn collect_body_text_from_render_plan(
    template: &Template,
    string_table: &StringTable,
) -> Vec<String> {
    let plan = template
        .render_plan
        .as_ref()
        .expect("parsed templates should carry a render plan");

    plan.pieces
        .iter()
        .filter_map(|piece| match piece {
            RenderPiece::Text(p) => Some(string_table.resolve(p.text).to_owned()),
            _ => None,
        })
        .collect()
}

fn collect_body_text_locations_from_render_plan(template: &Template) -> Vec<SourceLocation> {
    let plan = template
        .render_plan
        .as_ref()
        .expect("parsed templates should carry a render plan");

    plan.pieces
        .iter()
        .filter_map(|piece| match piece {
            RenderPiece::Text(p) => Some(p.location.to_owned()),
            _ => None,
        })
        .collect()
}

fn is_default_text_location(location: &SourceLocation) -> bool {
    location.scope == InternedPath::new()
        && location.start_pos == CharPosition::default()
        && location.end_pos == CharPosition::default()
}

fn is_default_error_location(
    location: &crate::compiler_frontend::compiler_errors::SourceLocation,
) -> bool {
    location.scope == InternedPath::new()
        && location.start_pos == CharPosition::default()
        && location.end_pos == CharPosition::default()
}

fn collect_static_template_fragments(
    atoms: &[TemplateAtom],
    string_table: &StringTable,
    output: &mut String,
) {
    for atom in atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        match &segment.expression.kind {
            ExpressionKind::StringSlice(value) => output.push_str(string_table.resolve(*value)),
            ExpressionKind::Template(template) => {
                collect_static_template_fragments(&template.content.atoms, string_table, output)
            }
            _ => {}
        }
    }
}

fn render_static_template_fragments(template: &Template, string_table: &StringTable) -> String {
    let mut rendered = String::new();
    collect_static_template_fragments(&template.content.atoms, string_table, &mut rendered);
    rendered
}

mod builder_tests;
mod children_tests;
mod code_tests;
mod directive_built_in_tests;
mod directive_style_tests;
mod head_tests;
mod markdown_tests;
mod render_tests;
mod slot_constant_tests;
mod slot_insert_tests;
mod whitespace_tests;
mod wrapper_tests;
