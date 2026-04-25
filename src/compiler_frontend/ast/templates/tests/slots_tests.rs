use super::*;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

fn template_tokens_from_source(source: &str, string_table: &mut StringTable) -> FileTokens {
    let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut tokens = tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
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

fn test_constant_context(scope: InternedPath) -> ScopeContext {
    let cwd = std::env::temp_dir();
    let resolver = ProjectPathResolver::new(cwd.clone(), cwd, &[])
        .expect("test path resolver should be valid");
    ScopeContext::new(
        ContextKind::Constant,
        scope.clone(),
        Rc::new(TopLevelDeclarationIndex::new(vec![])),
        HostRegistry::default(),
        vec![],
    )
    .with_project_path_resolver(Some(resolver))
    .with_source_file_scope(scope)
    .with_path_format_config(PathStringFormatConfig::default())
}

fn template_from_source(source: &str, string_table: &mut StringTable) -> Template {
    let mut tokens = template_tokens_from_source(source, string_table);
    let context = test_constant_context(tokens.src_path.to_owned());
    Template::new(&mut tokens, &context, Vec::new(), string_table).unwrap()
}

#[test]
fn test_parse_positional_slot() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$slot(1)]", &mut string_table);

    // Position at directive
    tokens.advance();

    let result = parse_slot_definition_target_argument(&mut tokens, &string_table);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SlotKey::Positional(1));
}

#[test]
fn test_parse_positional_slot_zero_errors() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$slot(0)]", &mut string_table);

    tokens.advance();

    let result = parse_slot_definition_target_argument(&mut tokens, &string_table);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .msg
            .contains("Positional slots start at 1")
    );
}

#[test]
fn test_parse_insert_positional_errors() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$insert(1)]", &mut string_table);

    tokens.advance();

    let result = parse_required_named_slot_insert_argument(&mut tokens, &string_table);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .msg
            .contains("only accepts quoted string literal names")
    );
}

#[test]
fn test_positional_composition_basic() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot(2)]]", &mut string_table);

    // Manually build fill content for isolation
    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result =
        compose_template_with_slots(&wrapper, fill_content, &location, &string_table).unwrap();

    // result should contain [a] and [b]
    assert_eq!(result.atoms.len(), 3); // "[a]", "-", "[b]"
    // The atoms for slots are expanded.
}

#[test]
fn test_positional_composition_with_default_overflow() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:c]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result =
        compose_template_with_slots(&wrapper, fill_content, &location, &string_table).unwrap();

    // [$slot(1)] should get [a]
    // [$slot] should get [b] and [c] (both are overflow)
    assert_eq!(result.atoms.len(), 4); // "[a]", "-", "[b]", "[c]"
}

#[test]
fn test_positional_composition_overflow_error() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result = compose_template_with_slots(&wrapper, fill_content, &location, &string_table);

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .msg
            .contains("more loose content than positional slots")
    );
}

#[test]
fn test_positional_composition_repeated_slots() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]and[$slot(1)]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            Expression::template(
                template_from_source("[:a]", &mut string_table),
                ValueMode::ImmutableOwned,
            ),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let location = SourceLocation::default();
    let result =
        compose_template_with_slots(&wrapper, fill_content, &location, &string_table).unwrap();

    // Both should get [a]
    assert_eq!(result.atoms.len(), 3); // "[a]", "and", "[a]"
}

#[test]
fn test_positional_composition_mixed_content() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]:[$slot]]", &mut string_table);

    // Mixed text and templates
    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::string_slice(
                    string_table.intern(" text "),
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result =
        compose_template_with_slots(&wrapper, fill_content, &location, &string_table).unwrap();

    // [a] -> [$slot(1)]
    // " text " and [b] -> [$slot]
    assert_eq!(result.atoms.len(), 4); // "[a]", ":", " text ", "[b]"
}
