//! Loop statement parsing helpers.
//!
//! WHAT: parses the three loop header forms (conditional, range, collection) and builds
//! fully-typed AST loop nodes with validated bindings.
//! WHY: loop headers now support richer syntax than the legacy `loop <binder> in ...` shape,
//! so parsing/validation needs one dedicated module with explicit helpers.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, LoopBindings, NodeKind, RangeEndKind, RangeLoopSpec,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_until,
};
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::condition_validation::ensure_boolean_condition;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::NestingDepth;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{ast_log, return_syntax_error};

const LOOP_PARSING_STAGE: &str = "Loop Parsing";

#[derive(Debug, Clone)]
struct ParsedBindingName {
    id: StringId,
    location: SourceLocation,
}

#[derive(Debug, Clone)]
struct ParsedBindingNames {
    item: ParsedBindingName,
    index: Option<ParsedBindingName>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum ParsedLoopHeader {
    Conditional {
        condition: Expression,
    },
    Range {
        bindings: LoopBindings,
        range: RangeLoopSpec,
    },
    Collection {
        bindings: LoopBindings,
        iterable: Expression,
    },
}

#[derive(Debug, Clone)]
struct BindingSuffixSplit {
    core_tokens: Vec<Token>,
    bindings: ParsedBindingNames,
}

pub fn create_loop(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    ast_log!("Creating a Loop");

    let location = token_stream.current_location();
    let scope = context.scope.clone();
    let colon_index = find_loop_header_colon_index(token_stream)?;

    let mut header_tokens = token_stream.tokens[token_stream.index..colon_index].to_vec();
    trim_edge_newlines(&mut header_tokens);

    if header_tokens.is_empty() {
        return_syntax_error!(
            "Loop header is empty. Expected a condition or iteration source after 'loop'",
            location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use syntax like 'loop is_ready():' or 'loop items |item|:'",
            }
        );
    }

    let (parsed_header, body_context) =
        parse_loop_header(&header_tokens, context, warnings, string_table)?;

    token_stream.index = colon_index + 1;
    let body = function_body_to_ast(token_stream, body_context, warnings, string_table)?;

    let kind = match parsed_header {
        ParsedLoopHeader::Conditional { condition } => NodeKind::WhileLoop(condition, body),
        ParsedLoopHeader::Range { bindings, range } => NodeKind::RangeLoop {
            bindings,
            range,
            body,
        },
        ParsedLoopHeader::Collection { bindings, iterable } => NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        },
    };

    Ok(AstNode {
        kind,
        location,
        scope,
    })
}

fn find_loop_header_colon_index(token_stream: &FileTokens) -> Result<usize, CompilerError> {
    let mut depth = NestingDepth::default();
    let mut index = token_stream.index;

    while index < token_stream.length {
        let token = &token_stream.tokens[index];
        let at_top_level = depth.is_top_level();

        if at_top_level && matches!(token.kind, TokenKind::Colon) {
            return Ok(index);
        }

        if at_top_level && matches!(token.kind, TokenKind::End | TokenKind::Eof) {
            return_syntax_error!(
                "A loop must have ':' after the loop header",
                token.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Add ':' after the loop condition or iteration header",
                    SuggestedInsertion => ":",
                }
            );
        }

        depth.step(&token.kind);
        index += 1;
    }

    return_syntax_error!(
        "A loop must have ':' after the loop header",
        token_stream.current_location(),
        {
            CompilationStage => LOOP_PARSING_STAGE,
            PrimarySuggestion => "Add ':' after the loop condition or iteration header",
            SuggestedInsertion => ":",
        }
    )
}

fn parse_loop_header(
    header_tokens: &[Token],
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<(ParsedLoopHeader, ScopeContext), CompilerError> {
    reject_removed_in_loop_syntax(header_tokens, string_table)?;

    // Range markers are syntax-defining for loop kind, so we dispatch range parsing first.
    // Non-range headers are then resolved as either conditional (`Bool`) or collection loops.
    if has_top_level_range_marker(header_tokens) {
        let parsed = parse_range_loop_header(header_tokens, &mut context, warnings, string_table)?;
        return Ok((parsed, context));
    }

    let parsed = parse_non_range_loop_header(header_tokens, &mut context, warnings, string_table)?;
    Ok((parsed, context))
}

fn parse_range_loop_header(
    header_tokens: &[Token],
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<ParsedLoopHeader, CompilerError> {
    // Try explicit binding suffixes first so diagnostics can point at malformed binding tails.
    if let Some(pipe_split) = parse_pipe_binding_suffix(header_tokens, string_table)? {
        let range =
            parse_range_loop_spec_from_tokens(&pipe_split.core_tokens, context, string_table)?;
        let binding_type = range_binding_type(&range, string_table)?;
        let bindings = declare_loop_bindings(
            Some(pipe_split.bindings),
            binding_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Range { bindings, range });
    }

    if let Some(dual_bare_split) = split_bare_dual_binding_suffix(header_tokens)
        && let Ok(range) =
            parse_range_loop_spec_from_tokens(&dual_bare_split.core_tokens, context, string_table)
    {
        let binding_type = range_binding_type(&range, string_table)?;
        let bindings = declare_loop_bindings(
            Some(dual_bare_split.bindings),
            binding_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Range { bindings, range });
    }

    if let Some(single_bare_split) = split_bare_single_binding_suffix(header_tokens)
        && let Ok(range) =
            parse_range_loop_spec_from_tokens(&single_bare_split.core_tokens, context, string_table)
    {
        let binding_type = range_binding_type(&range, string_table)?;
        let bindings = declare_loop_bindings(
            Some(single_bare_split.bindings),
            binding_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Range { bindings, range });
    }

    if has_trailing_dual_symbol_without_comma(header_tokens) {
        return_syntax_error!(
            "Missing comma between bare loop bindings",
            header_tokens[header_tokens.len() - 1].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use two bare bindings as 'value, index' or pipe form '|value, index|'",
            }
        );
    }

    let range = parse_range_loop_spec_from_tokens(header_tokens, context, string_table)?;
    let binding_type = range_binding_type(&range, string_table)?;
    let bindings = declare_loop_bindings(None, binding_type, context, warnings, string_table)?;
    Ok(ParsedLoopHeader::Range { bindings, range })
}

fn parse_non_range_loop_header(
    header_tokens: &[Token],
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<ParsedLoopHeader, CompilerError> {
    // Conditional loops are distinguished by a full-header boolean expression with no binding
    // suffix. We still probe binding suffixes first so malformed binding tails get targeted
    // diagnostics instead of a generic expression parse error.
    if let Some(pipe_split) = parse_pipe_binding_suffix(header_tokens, string_table)? {
        let (iterable, item_type) =
            parse_collection_iterable_from_tokens(&pipe_split.core_tokens, context, string_table)?;
        let bindings = declare_loop_bindings(
            Some(pipe_split.bindings),
            item_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Collection { bindings, iterable });
    }

    let full_expression = parse_expression_from_tokens(
        header_tokens,
        context,
        &Ownership::ImmutableOwned,
        string_table,
    );

    if let Some(dual_bare_split) = split_bare_dual_binding_suffix(header_tokens)
        && let Ok((iterable, item_type)) = parse_collection_iterable_from_tokens(
            &dual_bare_split.core_tokens,
            context,
            string_table,
        )
    {
        let bindings = declare_loop_bindings(
            Some(dual_bare_split.bindings),
            item_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Collection { bindings, iterable });
    }

    if has_trailing_dual_symbol_without_comma(header_tokens) {
        return_syntax_error!(
            "Missing comma between bare loop bindings",
            header_tokens[header_tokens.len() - 1].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use two bare bindings as 'item, index' or pipe form '|item, index|'",
            }
        );
    }

    if let Some(single_bare_split) = split_bare_single_binding_suffix(header_tokens)
        && let Ok((iterable, item_type)) = parse_collection_iterable_from_tokens(
            &single_bare_split.core_tokens,
            context,
            string_table,
        )
    {
        let bindings = declare_loop_bindings(
            Some(single_bare_split.bindings),
            item_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Collection { bindings, iterable });
    }

    match full_expression {
        Ok(expression) => {
            if let Some(item_type) = collection_element_type(&expression.data_type) {
                let bindings =
                    declare_loop_bindings(None, item_type, context, warnings, string_table)?;
                return Ok(ParsedLoopHeader::Collection {
                    bindings,
                    iterable: expression,
                });
            }

            ensure_boolean_condition(
                &expression,
                "Loop condition",
                &expression.location,
                LOOP_PARSING_STAGE,
                "Use a boolean expression after 'loop', e.g. loop is_ready():",
                string_table,
            )?;

            Ok(ParsedLoopHeader::Conditional {
                condition: expression,
            })
        }
        Err(error) => Err(error),
    }
}

fn reject_removed_in_loop_syntax(
    header_tokens: &[Token],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if header_tokens.len() < 3 {
        return Ok(());
    }

    let TokenKind::Symbol(_) = header_tokens[0].kind else {
        return Ok(());
    };

    let TokenKind::Symbol(second_symbol) = header_tokens[1].kind else {
        return Ok(());
    };

    if string_table.resolve(second_symbol) != "in" {
        return Ok(());
    }

    return_syntax_error!(
        "Old loop syntax 'loop <binder> in ...' was removed. Use 'loop 0 to 10 |i|:' or 'loop items |item|:'",
        header_tokens[1].location.clone(),
        {
            CompilationStage => LOOP_PARSING_STAGE,
            PrimarySuggestion => "Replace 'in' loops with the new header format: 'loop <condition> |bindings|:'",
        }
    )
}

fn parse_pipe_binding_suffix(
    header_tokens: &[Token],
    string_table: &StringTable,
) -> Result<Option<BindingSuffixSplit>, CompilerError> {
    let top_level_pipe_indexes = collect_top_level_token_indexes(header_tokens, |token| {
        matches!(token, TokenKind::TypeParameterBracket)
    });

    if top_level_pipe_indexes.is_empty() {
        return Ok(None);
    }

    if header_tokens
        .last()
        .is_none_or(|token| !matches!(token.kind, TokenKind::TypeParameterBracket))
    {
        return_syntax_error!(
            "Missing closing pipe in loop bindings",
            header_tokens[header_tokens.len() - 1].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Close loop bindings with '|', for example 'loop items |item|:'",
                SuggestedInsertion => "|",
            }
        );
    }

    if top_level_pipe_indexes.len() != 2 {
        return_syntax_error!(
            "Malformed loop binding pipes",
            header_tokens[top_level_pipe_indexes[0]].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use exactly one loop binding group, for example '|item|' or '|item, index|'",
            }
        );
    }

    let open_pipe_index = top_level_pipe_indexes[0];
    let close_pipe_index = top_level_pipe_indexes[1];

    if close_pipe_index <= open_pipe_index {
        return_syntax_error!(
            "Malformed loop binding pipes",
            header_tokens[open_pipe_index].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use loop bindings as '|item|' or '|item, index|'",
            }
        );
    }

    let core_tokens = header_tokens[..open_pipe_index].to_vec();
    let binding_tokens = header_tokens[open_pipe_index + 1..close_pipe_index].to_vec();

    if core_tokens.is_empty() {
        return_syntax_error!(
            "Loop header is missing a condition or iteration source before bindings",
            header_tokens[open_pipe_index].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Write the loop source before bindings, for example 'loop items |item|:'",
            }
        );
    }

    let bindings = parse_binding_tokens(&binding_tokens, string_table)?;

    Ok(Some(BindingSuffixSplit {
        core_tokens,
        bindings,
    }))
}

fn parse_binding_tokens(
    binding_tokens: &[Token],
    _string_table: &StringTable,
) -> Result<ParsedBindingNames, CompilerError> {
    let filtered_tokens = binding_tokens
        .iter()
        .filter(|token| !matches!(token.kind, TokenKind::Newline))
        .cloned()
        .collect::<Vec<_>>();

    if filtered_tokens.is_empty() {
        return_syntax_error!(
            "Loop binding list cannot be empty",
            binding_tokens
                .first()
                .map(|token| token.location.clone())
                .unwrap_or_default(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Add one or two bindings, for example '|item|' or '|item, index|'",
            }
        );
    }

    let mut names = Vec::with_capacity(2);
    let mut cursor = 0;

    while cursor < filtered_tokens.len() {
        let token = &filtered_tokens[cursor];
        let TokenKind::Symbol(symbol_id) = token.kind else {
            return_syntax_error!(
                "Loop bindings must be symbol names",
                token.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use symbol bindings like '|item|' or '|item, index|'",
                }
            );
        };

        names.push(ParsedBindingName {
            id: symbol_id,
            location: token.location.clone(),
        });
        cursor += 1;

        if cursor >= filtered_tokens.len() {
            break;
        }

        if !matches!(filtered_tokens[cursor].kind, TokenKind::Comma) {
            return_syntax_error!(
                "Missing comma between loop bindings",
                filtered_tokens[cursor].location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Separate loop bindings with commas, for example '|item, index|'",
                }
            );
        }

        cursor += 1;
        if cursor >= filtered_tokens.len() {
            return_syntax_error!(
                "Loop binding list cannot end with a comma",
                filtered_tokens[cursor - 1].location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Remove the trailing comma or add a second binding name",
                }
            );
        }
    }

    build_binding_name_pair(names)
}

fn split_bare_dual_binding_suffix(header_tokens: &[Token]) -> Option<BindingSuffixSplit> {
    // Bare dual form is intentionally strict: only `..., <item>, <index>` at header tail.
    if header_tokens.len() < 3 {
        return None;
    }

    let TokenKind::Symbol(index_id) = header_tokens[header_tokens.len() - 1].kind else {
        return None;
    };

    if !matches!(
        header_tokens[header_tokens.len() - 2].kind,
        TokenKind::Comma
    ) {
        return None;
    }

    let TokenKind::Symbol(item_id) = header_tokens[header_tokens.len() - 3].kind else {
        return None;
    };

    let core_tokens = header_tokens[..header_tokens.len() - 3].to_vec();
    if core_tokens.is_empty() {
        return None;
    }

    let bindings = ParsedBindingNames {
        item: ParsedBindingName {
            id: item_id,
            location: header_tokens[header_tokens.len() - 3].location.clone(),
        },
        index: Some(ParsedBindingName {
            id: index_id,
            location: header_tokens[header_tokens.len() - 1].location.clone(),
        }),
    };

    Some(BindingSuffixSplit {
        core_tokens,
        bindings,
    })
}

fn split_bare_single_binding_suffix(header_tokens: &[Token]) -> Option<BindingSuffixSplit> {
    // Bare single form accepts one trailing symbol and leaves all preceding tokens as header core.
    let TokenKind::Symbol(item_id) = header_tokens.last()?.kind else {
        return None;
    };

    let core_tokens = header_tokens[..header_tokens.len() - 1].to_vec();
    if core_tokens.is_empty() {
        return None;
    }

    if matches!(
        header_tokens[header_tokens.len() - 2].kind,
        TokenKind::Comma
    ) {
        return None;
    }

    let bindings = ParsedBindingNames {
        item: ParsedBindingName {
            id: item_id,
            location: header_tokens[header_tokens.len() - 1].location.clone(),
        },
        index: None,
    };

    Some(BindingSuffixSplit {
        core_tokens,
        bindings,
    })
}

fn has_trailing_dual_symbol_without_comma(header_tokens: &[Token]) -> bool {
    if header_tokens.len() < 3 {
        return false;
    }

    // Bare dual bindings must be `item, index`. This catches `item index` tails while avoiding
    // operator/field/call tails like `a + b value` and `thing.other value`.
    matches!(
        (
            &header_tokens[header_tokens.len() - 3].kind,
            &header_tokens[header_tokens.len() - 2].kind,
            &header_tokens[header_tokens.len() - 1].kind,
        ),
        (
            TokenKind::Symbol(_),
            TokenKind::Symbol(_),
            TokenKind::Symbol(_)
        )
    )
}

fn build_binding_name_pair(
    names: Vec<ParsedBindingName>,
) -> Result<ParsedBindingNames, CompilerError> {
    if names.len() > 2 {
        return_syntax_error!(
            "Loop bindings support at most two names",
            names[2].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use one binding for item/counter, or two for item/counter and index",
            }
        );
    }

    let Some(item) = names.first().cloned() else {
        return_syntax_error!(
            "Loop binding list cannot be empty",
            SourceLocation::default(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Add one or two loop bindings",
            }
        );
    };

    let index = names.get(1).cloned();

    if let Some(index_binding) = &index
        && index_binding.id == item.id
    {
        return_syntax_error!(
            "Duplicate loop binding name in the same loop header",
            index_binding.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use unique names for value and index bindings",
            }
        );
    }

    Ok(ParsedBindingNames { item, index })
}

fn parse_collection_iterable_from_tokens(
    iterable_tokens: &[Token],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(Expression, DataType), CompilerError> {
    let iterable = parse_expression_from_tokens(
        iterable_tokens,
        context,
        &Ownership::ImmutableReference,
        string_table,
    )?;

    let Some(item_type) = collection_element_type(&iterable.data_type) else {
        return_syntax_error!(
            format!(
                "Collection loop source must be a collection. Found '{}'",
                iterable.data_type.display_with_table(string_table)
            ),
            iterable.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                FoundType => iterable.data_type.display_with_table(string_table),
                ExpectedType => "Collection",
                PrimarySuggestion => "Use a collection expression before loop bindings, for example 'loop items |item|:'",
            }
        );
    };

    Ok((iterable, item_type))
}

fn parse_range_loop_spec_from_tokens(
    range_tokens: &[Token],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<RangeLoopSpec, CompilerError> {
    let mut range_stream = token_stream_with_eof(range_tokens)?;

    let mut start_type = DataType::Inferred;
    let start = create_expression_until(
        &mut range_stream,
        context,
        &mut start_type,
        &Ownership::ImmutableReference,
        &[
            TokenKind::ExclusiveRange,
            TokenKind::InclusiveRange,
            TokenKind::Eof,
        ],
        string_table,
    )?;

    let end_kind = match range_stream.current_token_kind() {
        TokenKind::ExclusiveRange => RangeEndKind::Exclusive,
        TokenKind::InclusiveRange => RangeEndKind::Inclusive,
        TokenKind::Eof => {
            return_syntax_error!(
                "Range loops must include 'to' or 'upto' between bounds",
                start.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use syntax like: loop 0 to 10 |i|:",
                    AlternativeSuggestion => "Use 'upto' for an inclusive end bound",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Range loops must include 'to' or 'upto' between bounds",
                range_stream.current_location(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use syntax like: loop 0 to 10 |i|:",
                    AlternativeSuggestion => "Use 'upto' for an inclusive end bound",
                }
            );
        }
    };

    range_stream.advance();

    if matches!(range_stream.current_token_kind(), TokenKind::Eof) {
        return_syntax_error!(
            "Range loop is missing an end bound",
            start.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Provide an end bound after 'to' or 'upto', for example 'loop 0 to 10 |i|:'",
            }
        );
    }

    let mut end_type = DataType::Inferred;
    let end = create_expression_until(
        &mut range_stream,
        context,
        &mut end_type,
        &Ownership::ImmutableReference,
        &[TokenKind::By, TokenKind::Eof],
        string_table,
    )?;

    let step = if matches!(range_stream.current_token_kind(), TokenKind::By) {
        let by_location = range_stream.current_location();
        range_stream.advance();

        if matches!(range_stream.current_token_kind(), TokenKind::Eof) {
            return_syntax_error!(
                "Range loop uses 'by' without a step value",
                by_location,
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Add a step after 'by', for example 'loop 0 to 10 by 2 |i|:'",
                }
            );
        }

        let mut step_type = DataType::Inferred;
        Some(create_expression_until(
            &mut range_stream,
            context,
            &mut step_type,
            &Ownership::ImmutableReference,
            &[TokenKind::Eof],
            string_table,
        )?)
    } else {
        None
    };

    let start_numeric = numeric_type_for_expression(&start).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range start must be numeric (Int or Float). Found '{}'",
                start.data_type.display_with_table(string_table)
            ),
            start.location.clone(),
        )
    })?;
    let end_numeric = numeric_type_for_expression(&end).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range end must be numeric (Int or Float). Found '{}'",
                end.data_type.display_with_table(string_table)
            ),
            end.location.clone(),
        )
    })?;

    let step_numeric = if let Some(step_expr) = &step {
        Some(numeric_type_for_expression(step_expr).ok_or_else(|| {
            CompilerError::new_syntax_error(
                format!(
                    "Range step must be numeric (Int or Float). Found '{}'",
                    step_expr.data_type.display_with_table(string_table)
                ),
                step_expr.location.clone(),
            )
        })?)
    } else {
        None
    };

    let uses_float = matches!(start_numeric, DataType::Float)
        || matches!(end_numeric, DataType::Float)
        || matches!(step_numeric, Some(DataType::Float));

    if uses_float && step.is_none() {
        return_syntax_error!(
            "Float ranges require an explicit 'by' step",
            end.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Add an explicit step, for example 'loop 0.0 to 1.0 by 0.1 |t|:'",
            }
        );
    }

    if let Some(step_expr) = &step
        && is_zero_numeric_literal(step_expr)
    {
        return_syntax_error!(
            "Range step cannot be zero",
            step_expr.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use a non-zero step value after 'by'",
            }
        );
    }

    Ok(RangeLoopSpec {
        start,
        end,
        end_kind,
        step,
    })
}

fn range_binding_type(
    range: &RangeLoopSpec,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    let start_numeric = numeric_type_for_expression(&range.start).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range start must be numeric (Int or Float). Found '{}'",
                range.start.data_type.display_with_table(string_table)
            ),
            range.start.location.clone(),
        )
    })?;

    let end_numeric = numeric_type_for_expression(&range.end).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range end must be numeric (Int or Float). Found '{}'",
                range.end.data_type.display_with_table(string_table)
            ),
            range.end.location.clone(),
        )
    })?;

    let step_numeric = if let Some(step_expr) = &range.step {
        Some(numeric_type_for_expression(step_expr).ok_or_else(|| {
            CompilerError::new_syntax_error(
                format!(
                    "Range step must be numeric (Int or Float). Found '{}'",
                    step_expr.data_type.display_with_table(string_table)
                ),
                step_expr.location.clone(),
            )
        })?)
    } else {
        None
    };

    let uses_float = matches!(start_numeric, DataType::Float)
        || matches!(end_numeric, DataType::Float)
        || matches!(step_numeric, Some(DataType::Float));

    Ok(if uses_float {
        DataType::Float
    } else {
        DataType::Int
    })
}

fn declare_loop_bindings(
    names: Option<ParsedBindingNames>,
    item_data_type: DataType,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<LoopBindings, CompilerError> {
    let Some(names) = names else {
        return Ok(LoopBindings {
            item: None,
            index: None,
        });
    };

    let item = Some(declare_loop_binding(
        &names.item,
        item_data_type,
        context,
        warnings,
        string_table,
    )?);

    let index = names
        .index
        .as_ref()
        .map(|index_name| {
            declare_loop_binding(index_name, DataType::Int, context, warnings, string_table)
        })
        .transpose()?;

    Ok(LoopBindings { item, index })
}

fn declare_loop_binding(
    name: &ParsedBindingName,
    data_type: DataType,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let binding_name_text = string_table.resolve(name.id).to_owned();

    ensure_not_keyword_shadow_identifier(
        &binding_name_text,
        name.location.clone(),
        LOOP_PARSING_STAGE,
    )?;

    if context.get_reference(&name.id).is_some() {
        return_syntax_error!(
            format!(
                "Loop binding '{}' is already declared in this scope",
                binding_name_text
            ),
            name.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use a different binding name. Shadowing is not supported",
            }
        );
    }

    if let Some(warning) = naming_warning_for_identifier(
        &binding_name_text,
        name.location.clone(),
        IdentifierNamingKind::ValueLike,
    ) {
        warnings.push(warning);
    }

    let declaration = Declaration {
        id: context.scope.append(name.id),
        value: Expression::new(
            ExpressionKind::NoValue,
            name.location.clone(),
            data_type,
            Ownership::ImmutableOwned,
        ),
    };

    context.add_var(declaration.to_owned());
    Ok(declaration)
}

fn parse_expression_from_tokens(
    expression_tokens: &[Token],
    context: &ScopeContext,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut scoped_stream = token_stream_with_eof(expression_tokens)?;
    let mut inferred_type = DataType::Inferred;

    create_expression(
        &mut scoped_stream,
        context,
        &mut inferred_type,
        ownership,
        false,
        string_table,
    )
}

fn token_stream_with_eof(tokens: &[Token]) -> Result<FileTokens, CompilerError> {
    if tokens.is_empty() {
        return_syntax_error!(
            "Expected an expression in loop header",
            SourceLocation::default(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Add a condition or iteration source after 'loop'",
            }
        );
    }

    let mut scoped_tokens = tokens.to_vec();
    let eof_location = tokens[tokens.len() - 1].location.clone();
    let src_path = tokens[0].location.scope.clone();

    scoped_tokens.push(Token::new(TokenKind::Eof, eof_location));

    Ok(FileTokens::new(src_path, scoped_tokens))
}

fn collection_element_type(data_type: &DataType) -> Option<DataType> {
    match data_type {
        DataType::Collection(inner, _) => Some((**inner).clone()),
        DataType::Reference(inner) => collection_element_type(inner),
        _ => None,
    }
}

fn has_top_level_range_marker(tokens: &[Token]) -> bool {
    let mut depth = NestingDepth::default();

    for token in tokens {
        if depth.is_top_level()
            && matches!(
                token.kind,
                TokenKind::ExclusiveRange | TokenKind::InclusiveRange
            )
        {
            return true;
        }

        depth.step(&token.kind);
    }

    false
}

fn collect_top_level_token_indexes(
    tokens: &[Token],
    matches_token: impl Fn(&TokenKind) -> bool,
) -> Vec<usize> {
    let mut depth = NestingDepth::default();
    let mut indexes = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if depth.is_top_level() && matches_token(&token.kind) {
            indexes.push(index);
        }

        depth.step(&token.kind);
    }

    indexes
}

fn trim_edge_newlines(tokens: &mut Vec<Token>) {
    while tokens
        .first()
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        tokens.remove(0);
    }

    while tokens
        .last()
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        tokens.pop();
    }
}

fn numeric_type_for_expression(expression: &Expression) -> Option<DataType> {
    numeric_type_from_datatype(&expression.data_type)
}

fn numeric_type_from_datatype(data_type: &DataType) -> Option<DataType> {
    match data_type {
        DataType::Int => Some(DataType::Int),
        DataType::Float => Some(DataType::Float),
        DataType::Reference(inner) => numeric_type_from_datatype(inner),
        _ => None,
    }
}

fn is_zero_numeric_literal(expression: &Expression) -> bool {
    match expression.kind {
        ExpressionKind::Int(value) => value == 0,
        ExpressionKind::Float(value) => value == 0.0,
        _ => false,
    }
}

#[cfg(test)]
#[path = "tests/loop_parsing_tests.rs"]
mod loop_parsing_tests;
