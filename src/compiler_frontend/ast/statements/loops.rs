//! Loop statement parsing helpers.
//!
//! WHAT: parses the three loop header forms (conditional, range, collection) and builds
//! fully-typed AST loop nodes with validated bindings.
//! WHY: loop headers now support richer syntax than the legacy `loop <binder> in ...` shape,
//! so parsing/validation needs one dedicated module with explicit helpers.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, LoopBindings, NodeKind, RangeEndKind, RangeLoopSpec,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_until,
};
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::condition_validation::ensure_loop_condition;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::NestingDepth;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BareLoopBindingKind {
    Single,
    Dual,
}

#[derive(Debug, Clone)]
struct BareLoopBindingSuffix {
    core_tokens: Vec<Token>,
    location: SourceLocation,
    kind: BareLoopBindingKind,
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

    let (parsed_loop_header, body_context) =
        parse_loop_header(&header_tokens, context, warnings, string_table)?;

    token_stream.index = colon_index + 1;
    let body = function_body_to_ast(token_stream, body_context, warnings, string_table)?;

    let kind = match parsed_loop_header {
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
    let mut nesting_depth = NestingDepth::default();
    let mut search_index = token_stream.index;

    while search_index < token_stream.length {
        let token = &token_stream.tokens[search_index];
        let is_top_level = nesting_depth.is_top_level();

        if is_top_level && matches!(token.kind, TokenKind::Colon) {
            return Ok(search_index);
        }

        if is_top_level && matches!(token.kind, TokenKind::End | TokenKind::Eof) {
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

        nesting_depth.step(&token.kind);
        search_index += 1;
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
        let loop_header =
            parse_range_loop_header(header_tokens, &mut context, warnings, string_table)?;
        return Ok((loop_header, context));
    }

    let loop_header =
        parse_non_range_loop_header(header_tokens, &mut context, warnings, string_table)?;
    Ok((loop_header, context))
}

fn parse_range_loop_header(
    header_tokens: &[Token],
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<ParsedLoopHeader, CompilerError> {
    // Parse explicit `|...|` bindings first, then reject bare binding tails with a targeted
    // diagnostic before falling back to no-binding range parsing.
    if let Some(pipe_binding_split) = parse_pipe_binding_suffix(header_tokens, string_table)? {
        let range = parse_range_loop_spec_from_tokens(
            &pipe_binding_split.core_tokens,
            context,
            string_table,
        )?;
        let binding_type = range_binding_type(&range, string_table)?;
        let bindings = declare_loop_bindings(
            Some(pipe_binding_split.bindings),
            binding_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Range { bindings, range });
    }

    if let Some(bare_binding_suffix) = detect_bare_loop_binding_suffix(header_tokens)
        && parse_range_loop_spec_from_tokens(
            &bare_binding_suffix.core_tokens,
            context,
            string_table,
        )
        .is_ok()
    {
        return bare_loop_binding_syntax_error(&bare_binding_suffix);
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
    // suffix. Parse explicit `|...|` bindings first, then reject bare binding tails with a
    // targeted diagnostic before evaluating conditional/collection fallback.
    if let Some(pipe_binding_split) = parse_pipe_binding_suffix(header_tokens, string_table)? {
        let (iterable, item_type) = parse_collection_iterable_from_tokens(
            &pipe_binding_split.core_tokens,
            context,
            string_table,
        )?;
        let bindings = declare_loop_bindings(
            Some(pipe_binding_split.bindings),
            item_type,
            context,
            warnings,
            string_table,
        )?;

        return Ok(ParsedLoopHeader::Collection { bindings, iterable });
    }

    if let Some(bare_binding_suffix) = detect_bare_loop_binding_suffix(header_tokens)
        && parses_as_collection_iterable(&bare_binding_suffix.core_tokens, context, string_table)
    {
        return bare_loop_binding_syntax_error(&bare_binding_suffix);
    }

    let header_expression = parse_expression_from_tokens(
        header_tokens,
        context,
        &ValueMode::ImmutableOwned,
        string_table,
    );

    match header_expression {
        Ok(expression) => {
            if let Some(item_type) = collection_element_type(&expression.data_type) {
                let bindings =
                    declare_loop_bindings(None, item_type, context, warnings, string_table)?;
                return Ok(ParsedLoopHeader::Collection {
                    bindings,
                    iterable: expression,
                });
            }

            ensure_loop_condition(&expression, string_table)?;

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
    let pipe_indices = collect_top_level_token_indexes(header_tokens, |token| {
        matches!(token, TokenKind::TypeParameterBracket)
    });

    if pipe_indices.is_empty() {
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

    if pipe_indices.len() != 2 {
        return_syntax_error!(
            "Malformed loop binding pipes",
            header_tokens[pipe_indices[0]].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use exactly one loop binding group, for example '|item|' or '|item, index|'",
            }
        );
    }

    let open_pipe_index = pipe_indices[0];
    let close_pipe_index = pipe_indices[1];

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

    let mut binding_names = Vec::with_capacity(2);
    let mut position = 0;

    while position < filtered_tokens.len() {
        let token = &filtered_tokens[position];
        if token.kind == TokenKind::This {
            return_syntax_error!(
                "'this' is reserved for method receiver parameters and cannot be used as a loop binding.",
                token.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Choose a different name for the loop binding",
                }
            );
        }
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

        binding_names.push(ParsedBindingName {
            id: symbol_id,
            location: token.location.clone(),
        });
        position += 1;

        if position >= filtered_tokens.len() {
            break;
        }

        if !matches!(filtered_tokens[position].kind, TokenKind::Comma) {
            return_syntax_error!(
                "Missing comma between loop bindings",
                filtered_tokens[position].location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Separate loop bindings with commas, for example '|item, index|'",
                }
            );
        }

        position += 1;
        if position >= filtered_tokens.len() {
            return_syntax_error!(
                "Loop binding list cannot end with a comma",
                filtered_tokens[position - 1].location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Remove the trailing comma or add a second binding name",
                }
            );
        }
    }

    build_binding_name_pair(binding_names)
}

fn detect_bare_loop_binding_suffix(header_tokens: &[Token]) -> Option<BareLoopBindingSuffix> {
    let non_newline_indices = collect_top_level_token_indexes(header_tokens, |token| {
        !matches!(token, TokenKind::Newline)
    });
    if non_newline_indices.len() < 2 {
        return None;
    }

    if non_newline_indices.len() >= 3 {
        let first_index = non_newline_indices[non_newline_indices.len() - 3];
        let separator_index = non_newline_indices[non_newline_indices.len() - 2];
        let second_index = non_newline_indices[non_newline_indices.len() - 1];

        if matches!(header_tokens[first_index].kind, TokenKind::Symbol(_))
            && matches!(header_tokens[separator_index].kind, TokenKind::Comma)
            && matches!(header_tokens[second_index].kind, TokenKind::Symbol(_))
            && first_index > 0
        {
            return Some(BareLoopBindingSuffix {
                core_tokens: header_tokens[..first_index].to_vec(),
                location: header_tokens[first_index].location.clone(),
                kind: BareLoopBindingKind::Dual,
            });
        }

        if matches!(header_tokens[first_index].kind, TokenKind::Symbol(_))
            && matches!(header_tokens[separator_index].kind, TokenKind::Symbol(_))
            && matches!(header_tokens[second_index].kind, TokenKind::Symbol(_))
            && separator_index > 0
        {
            return Some(BareLoopBindingSuffix {
                core_tokens: header_tokens[..separator_index].to_vec(),
                location: header_tokens[separator_index].location.clone(),
                kind: BareLoopBindingKind::Dual,
            });
        }
    }

    let binding_index = *non_newline_indices.last()?;
    let core_tail_index = non_newline_indices[non_newline_indices.len() - 2];

    if matches!(header_tokens[binding_index].kind, TokenKind::Symbol(_))
        && !matches!(header_tokens[core_tail_index].kind, TokenKind::Comma)
    {
        return Some(BareLoopBindingSuffix {
            core_tokens: header_tokens[..binding_index].to_vec(),
            location: header_tokens[binding_index].location.clone(),
            kind: BareLoopBindingKind::Single,
        });
    }

    None
}

fn parses_as_collection_iterable(
    iterable_tokens: &[Token],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> bool {
    let Ok(iterable_expression) = parse_expression_from_tokens(
        iterable_tokens,
        context,
        &ValueMode::ImmutableReference,
        string_table,
    ) else {
        return false;
    };

    collection_element_type(&iterable_expression.data_type).is_some()
}

fn bare_loop_binding_syntax_error<T>(
    binding_suffix: &BareLoopBindingSuffix,
) -> Result<T, CompilerError> {
    match binding_suffix.kind {
        BareLoopBindingKind::Single => {
            return_syntax_error!(
                "Loop bindings must use `|...|` after the loop source or range.",
                binding_suffix.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use syntax like `loop items |item|:` or `loop 0 to 10 |i|:`",
                }
            )
        }
        BareLoopBindingKind::Dual => {
            return_syntax_error!(
                "Loop bindings must use `|item, index|` form.",
                binding_suffix.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Write `loop items |item, index|:` instead of bare trailing names.",
                    AlternativeSuggestion => "Range loops use the same shape, e.g. `loop 0 to 10 |value, index|:`",
                }
            )
        }
    }
}

fn build_binding_name_pair(
    binding_names: Vec<ParsedBindingName>,
) -> Result<ParsedBindingNames, CompilerError> {
    if binding_names.len() > 2 {
        return_syntax_error!(
            "Loop bindings support at most two names",
            binding_names[2].location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use one binding for item/counter, or two for item/counter and index",
            }
        );
    }

    let Some(item_binding) = binding_names.first().cloned() else {
        return_syntax_error!(
            "Loop binding list cannot be empty",
            SourceLocation::default(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Add one or two loop bindings",
            }
        );
    };

    let index_binding = binding_names.get(1).cloned();

    if let Some(index) = &index_binding
        && index.id == item_binding.id
    {
        return_syntax_error!(
            "Duplicate loop binding name in the same loop header",
            index.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use unique names for value and index bindings",
            }
        );
    }

    Ok(ParsedBindingNames {
        item: item_binding,
        index: index_binding,
    })
}

fn parse_collection_iterable_from_tokens(
    iterable_tokens: &[Token],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(Expression, DataType), CompilerError> {
    let collection_expression = parse_expression_from_tokens(
        iterable_tokens,
        context,
        &ValueMode::ImmutableReference,
        string_table,
    )?;

    let Some(item_type) = collection_element_type(&collection_expression.data_type) else {
        return_syntax_error!(
            format!(
                "Collection loop source must be a collection. Found '{}'",
                collection_expression.data_type.display_with_table(string_table)
            ),
            collection_expression.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                FoundType => collection_expression.data_type.display_with_table(string_table),
                ExpectedType => "Collection",
                PrimarySuggestion => "Use a collection expression before loop bindings, for example 'loop items |item|:'",
            }
        );
    };

    Ok((collection_expression, item_type))
}

fn parse_range_loop_spec_from_tokens(
    range_tokens: &[Token],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<RangeLoopSpec, CompilerError> {
    let mut stream = token_stream_with_eof(range_tokens)?;

    // Omitted-start sugar: `loop to 5:` desugars to `loop 0 to 5:`.
    let start = if matches!(stream.current_token_kind(), TokenKind::ExclusiveRange) {
        let location = stream.current_location();
        Expression::new(
            ExpressionKind::Int(0),
            location,
            DataType::Int,
            ValueMode::ImmutableOwned,
        )
    } else {
        let mut start_type = DataType::Inferred;
        create_expression_until(
            &mut stream,
            context,
            &mut start_type,
            &ValueMode::ImmutableReference,
            &[TokenKind::ExclusiveRange, TokenKind::Eof],
            string_table,
        )?
    };

    let end_kind = match stream.current_token_kind() {
        TokenKind::ExclusiveRange => {
            stream.advance();
            // Optional inclusive marker: `to & end`
            if matches!(stream.current_token_kind(), TokenKind::Ampersand) {
                stream.advance();
                RangeEndKind::Inclusive
            } else {
                RangeEndKind::Exclusive
            }
        }
        TokenKind::Eof => {
            return_syntax_error!(
                "Range loops must include 'to' between bounds",
                start.location.clone(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use syntax like: loop 0 to 10 |i|:",
                    AlternativeSuggestion => "Use `to & end` for an inclusive end bound",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Range loops must include 'to' between bounds",
                stream.current_location(),
                {
                    CompilationStage => LOOP_PARSING_STAGE,
                    PrimarySuggestion => "Use syntax like: loop 0 to 10 |i|:",
                    AlternativeSuggestion => "Use `to & end` for an inclusive end bound",
                }
            );
        }
    };

    if matches!(stream.current_token_kind(), TokenKind::Eof) {
        return_syntax_error!(
            "Range loop is missing an end bound",
            start.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Provide an end bound after 'to', for example 'loop 0 to 10 |i|:'",
            }
        );
    }

    let mut end_type = DataType::Inferred;
    let end = create_expression_until(
        &mut stream,
        context,
        &mut end_type,
        &ValueMode::ImmutableReference,
        &[TokenKind::By, TokenKind::Eof],
        string_table,
    )?;

    let step = if matches!(stream.current_token_kind(), TokenKind::By) {
        let by_location = stream.current_location();
        stream.advance();

        if matches!(stream.current_token_kind(), TokenKind::Eof) {
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
            &mut stream,
            context,
            &mut step_type,
            &ValueMode::ImmutableReference,
            &[TokenKind::Eof],
            string_table,
        )?)
    } else {
        None
    };

    let start_number_type = numeric_type_for_expression(&start).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range start must be numeric (Int or Float). Found '{}'",
                start.data_type.display_with_table(string_table)
            ),
            start.location.clone(),
        )
    })?;
    let end_number_type = numeric_type_for_expression(&end).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range end must be numeric (Int or Float). Found '{}'",
                end.data_type.display_with_table(string_table)
            ),
            end.location.clone(),
        )
    })?;

    let step_number_type = if let Some(step_expression) = &step {
        Some(numeric_type_for_expression(step_expression).ok_or_else(|| {
            CompilerError::new_syntax_error(
                format!(
                    "Range step must be numeric (Int or Float). Found '{}'",
                    step_expression.data_type.display_with_table(string_table)
                ),
                step_expression.location.clone(),
            )
        })?)
    } else {
        None
    };

    let uses_float = matches!(start_number_type, DataType::Float)
        || matches!(end_number_type, DataType::Float)
        || matches!(step_number_type, Some(DataType::Float));

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

    if let Some(step_expression) = &step
        && is_zero_numeric_literal(step_expression)
    {
        return_syntax_error!(
            "Range step cannot be zero",
            step_expression.location.clone(),
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
    let start_number_type = numeric_type_for_expression(&range.start).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range start must be numeric (Int or Float). Found '{}'",
                range.start.data_type.display_with_table(string_table)
            ),
            range.start.location.clone(),
        )
    })?;

    let end_number_type = numeric_type_for_expression(&range.end).ok_or_else(|| {
        CompilerError::new_syntax_error(
            format!(
                "Range end must be numeric (Int or Float). Found '{}'",
                range.end.data_type.display_with_table(string_table)
            ),
            range.end.location.clone(),
        )
    })?;

    let step_number_type = if let Some(step_expression) = &range.step {
        Some(numeric_type_for_expression(step_expression).ok_or_else(|| {
            CompilerError::new_syntax_error(
                format!(
                    "Range step must be numeric (Int or Float). Found '{}'",
                    step_expression.data_type.display_with_table(string_table)
                ),
                step_expression.location.clone(),
            )
        })?)
    } else {
        None
    };

    let uses_float = matches!(start_number_type, DataType::Float)
        || matches!(end_number_type, DataType::Float)
        || matches!(step_number_type, Some(DataType::Float));

    Ok(if uses_float {
        DataType::Float
    } else {
        DataType::Int
    })
}

fn declare_loop_bindings(
    binding_names: Option<ParsedBindingNames>,
    item_data_type: DataType,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<LoopBindings, CompilerError> {
    let Some(binding_names) = binding_names else {
        return Ok(LoopBindings {
            item: None,
            index: None,
        });
    };

    let item = Some(declare_loop_binding(
        &binding_names.item,
        item_data_type,
        context,
        warnings,
        string_table,
    )?);

    let index = binding_names
        .index
        .as_ref()
        .map(|index_name| {
            declare_loop_binding(index_name, DataType::Int, context, warnings, string_table)
        })
        .transpose()?;

    Ok(LoopBindings { item, index })
}

fn declare_loop_binding(
    binding_name: &ParsedBindingName,
    data_type: DataType,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let name_text = string_table.resolve(binding_name.id).to_owned();

    ensure_not_keyword_shadow_identifier(
        &name_text,
        binding_name.location.clone(),
        LOOP_PARSING_STAGE,
    )?;

    if context.get_reference(&binding_name.id).is_some() {
        return_syntax_error!(
            format!(
                "Loop binding '{}' is already declared in this scope",
                name_text
            ),
            binding_name.location.clone(),
            {
                CompilationStage => LOOP_PARSING_STAGE,
                PrimarySuggestion => "Use a different binding name. Shadowing is not supported",
            }
        );
    }

    if let Some(warning) = naming_warning_for_identifier(
        &name_text,
        binding_name.location.clone(),
        IdentifierNamingKind::ValueLike,
    ) {
        warnings.push(warning);
    }

    let declaration = Declaration {
        id: context.scope.append(binding_name.id),
        value: Expression::new(
            ExpressionKind::NoValue,
            binding_name.location.clone(),
            data_type,
            ValueMode::ImmutableOwned,
        ),
    };

    context.add_var(declaration.to_owned());
    Ok(declaration)
}

fn parse_expression_from_tokens(
    expression_tokens: &[Token],
    context: &ScopeContext,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut expression_stream = token_stream_with_eof(expression_tokens)?;
    let mut inferred_type = DataType::Inferred;

    create_expression(
        &mut expression_stream,
        context,
        &mut inferred_type,
        value_mode,
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

    let mut tokens_with_eof = tokens.to_vec();
    let eof_location = tokens[tokens.len() - 1].location.clone();
    let src_path = tokens[0].location.scope.clone();

    tokens_with_eof.push(Token::new(TokenKind::Eof, eof_location));

    Ok(FileTokens::new(src_path, tokens_with_eof))
}

fn collection_element_type(collection_type: &DataType) -> Option<DataType> {
    match collection_type {
        collection_type if collection_type.is_collection() => {
            collection_type.collection_element_type_cloned()
        }
        DataType::Reference(inner) => collection_element_type(inner),
        _ => None,
    }
}

fn has_top_level_range_marker(tokens: &[Token]) -> bool {
    let mut nesting_depth = NestingDepth::default();

    for token in tokens {
        if nesting_depth.is_top_level() && matches!(token.kind, TokenKind::ExclusiveRange) {
            return true;
        }

        nesting_depth.step(&token.kind);
    }

    false
}

fn collect_top_level_token_indexes(
    tokens: &[Token],
    predicate: impl Fn(&TokenKind) -> bool,
) -> Vec<usize> {
    let mut nesting_depth = NestingDepth::default();
    let mut indexes = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if nesting_depth.is_top_level() && predicate(&token.kind) {
            indexes.push(index);
        }

        nesting_depth.step(&token.kind);
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
