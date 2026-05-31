//! Template body parsing.
//!
//! WHAT: Parses the body section of a template — string tokens, nested child
//! templates, slot definitions, and newlines — in source order.
//!
//! WHY: Separates body token consumption from head parsing and composition,
//! keeping each parsing phase focused and testable.

#![allow(clippy::result_large_err)]
use crate::ast_log;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::if_headers::{ParsedIfHeader, parse_if_header};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateParsingMode, TemplateSegment,
    TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_sentinels::{
    DirectLoopControlMarker, ElseSentinelPolicy, TemplateBodyBoundary, TemplateBodyControlContext,
    classify_direct_else_marker, classify_direct_loop_control_marker,
    ensure_else_boundary_after_sentinel, ensure_else_content_starts_on_new_boundary,
    ensure_loop_control_boundary_after_sentinel, ensure_loop_control_boundary_before_sentinel,
    handle_direct_else_marker, loop_control_marker_close_index, loop_control_marker_location,
    malformed_loop_control_reason, orphan_loop_control_diagnostic, remap_else_if_inline_diagnostic,
    trim_leading_whitespace_atoms, trim_trailing_whitespace_atoms,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBodyParseMode, TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch,
    TemplateControlFlow, TemplateControlFlowValidationMode, TemplateFallbackBranch,
    TemplateIfBodyParseInput, TemplateLoopBodyParseInput, TemplateLoopControlFlow,
    TemplateLoopControlKind, TemplateLoopControlSignal,
    inline_source_consts_for_const_required_if_condition,
};
use crate::compiler_frontend::ast::templates::template_types::{Template, TemplateInheritance};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::consume_balanced_template_region;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Body Parser Entry
// -------------------------

/// Parses the body section of a template, consuming tokens until the closing
/// delimiter or EOF. Nested child templates are recursively parsed.
///
/// `foldable` is set to `false` if a runtime (non-const) child template is
/// encountered.
pub(crate) fn parse_template_body(
    token_stream: &mut FileTokens,
    template: &mut Template,
    input: TemplateBodyParseRequest<'_, '_>,
) -> Result<(), CompilerDiagnostic> {
    let TemplateBodyParseRequest {
        context,
        type_interner,
        body_mode,
        direct_child_wrappers,
        control_flow_validation,
        control_context,
        foldable,
        string_table,
    } = input;

    let mut parser = TemplateBodyParser {
        token_stream,
        type_interner,
        direct_child_wrappers,
        control_flow_validation,
        foldable,
        string_table,
    };

    match body_mode {
        TemplateBodyParseMode::Normal => parser
            .parse_content(
                context,
                template,
                control_context.with_else_policy(ElseSentinelPolicy::Orphan),
                InheritedChildWrapperPolicy::Apply,
            )
            .map(|_| ()),

        TemplateBodyParseMode::If(input) => parser.parse_if_body(template, *input, control_context),

        TemplateBodyParseMode::Loop(input) => {
            parser.parse_loop_body(template, *input, control_context)
        }
    }
}

/// Shared input bundle for one template body parse.
///
/// WHAT: carries the mutable AST/body parser services used by every recursive
/// body mode.
/// WHY: control-flow body parsing needs the same token/type/string-table state
/// across normal, if, and loop paths without threading long argument lists.
pub(crate) struct TemplateBodyParseRequest<'a, 'types> {
    pub(crate) context: &'a ScopeContext,
    pub(crate) type_interner: &'a mut AstTypeInterner<'types>,
    pub(crate) body_mode: TemplateBodyParseMode,
    pub(crate) direct_child_wrappers: &'a [Template],
    pub(crate) control_flow_validation: TemplateControlFlowValidationMode,
    pub(crate) control_context: TemplateBodyControlContext,
    pub(crate) foldable: &'a mut bool,
    pub(crate) string_table: &'a mut StringTable,
}

/// Options that stay stable for one template node while its head and body are parsed.
///
/// WHAT: groups doc-comment mode, runtime/const validation mode, and inherited
/// body-control state for recursive template construction.
/// WHY: nested template parsing needs these three values together, and grouping
/// them keeps `Template::new_nested_template` from becoming a long argument list.
#[derive(Clone, Copy)]
pub(crate) struct NestedTemplateParseOptions {
    pub(crate) parsing_mode: TemplateParsingMode,
    pub(crate) control_flow_validation: TemplateControlFlowValidationMode,
    pub(crate) control_context: TemplateBodyControlContext,
}

impl NestedTemplateParseOptions {
    pub(crate) fn runtime_capable() -> Self {
        Self {
            parsing_mode: TemplateParsingMode::Standard,
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            control_context: TemplateBodyControlContext::normal(),
        }
    }

    pub(crate) fn const_required() -> Self {
        Self {
            parsing_mode: TemplateParsingMode::Standard,
            control_flow_validation: TemplateControlFlowValidationMode::ConstRequired,
            control_context: TemplateBodyControlContext::normal(),
        }
    }
}

struct TemplateBodyParser<'a, 'types> {
    token_stream: &'a mut FileTokens,
    type_interner: &'a mut AstTypeInterner<'types>,
    direct_child_wrappers: &'a [Template],
    control_flow_validation: TemplateControlFlowValidationMode,
    foldable: &'a mut bool,
    string_table: &'a mut StringTable,
}

impl<'a, 'types> TemplateBodyParser<'a, 'types> {
    fn parse_content(
        &mut self,
        context: &ScopeContext,
        template: &mut Template,
        control_context: TemplateBodyControlContext,
        inherited_wrappers: InheritedChildWrapperPolicy,
    ) -> Result<TemplateBodyBoundary, CompilerDiagnostic> {
        // The tokenizer only allows for strings, templates or slots inside the template body.
        while self.token_stream.index < self.token_stream.tokens.len() {
            let token_kind = self.token_stream.current_token_kind().clone();

            match token_kind {
                TokenKind::Eof => {
                    return Ok(TemplateBodyBoundary::Eof);
                }

                TokenKind::TemplateClose => {
                    ast_log!("Breaking out of template body. Found a template close.");
                    // Need to skip the closer
                    self.token_stream.advance();
                    return Ok(TemplateBodyBoundary::TemplateClose);
                }

                TokenKind::TemplateHead => {
                    if let Some(else_marker) = classify_direct_else_marker(self.token_stream) {
                        return handle_direct_else_marker(
                            self.token_stream,
                            template,
                            else_marker,
                            control_context.else_policy,
                            self.string_table,
                        );
                    }

                    if let Some(loop_marker) =
                        classify_direct_loop_control_marker(self.token_stream)
                    {
                        self.handle_loop_control_marker(template, &loop_marker, control_context)?;
                        continue;
                    }

                    // When child templates are suppressed (e.g. `$doc`), brackets are
                    // treated as balanced literal text rather than parsed as nested templates.
                    if template.style.suppress_child_templates {
                        consume_balanced_brackets_as_literal_text(
                            self.token_stream,
                            template,
                            self.string_table,
                        );
                        continue;
                    }

                    self.parse_nested_template(
                        context,
                        template,
                        control_context,
                        inherited_wrappers,
                    )?;
                    continue;
                }

                TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                    template.content.add(Expression::string_slice(
                        content,
                        self.token_stream.current_location(),
                        ValueMode::ImmutableOwned,
                    ));
                }

                TokenKind::Newline => {
                    let newline_id = self.string_table.intern("\n");
                    template.content.add(Expression::string_slice(
                        newline_id,
                        self.token_stream.current_location(),
                        ValueMode::ImmutableOwned,
                    ));
                }

                found => {
                    return Err(CompilerDiagnostic::unexpected_token(
                        found,
                        self.token_stream.current_location(),
                    ));
                }
            }

            self.token_stream.advance();
        }

        Ok(TemplateBodyBoundary::Eof)
    }

    fn parse_if_body(
        &mut self,
        template: &mut Template,
        input: TemplateIfBodyParseInput,
        control_context: TemplateBodyControlContext,
    ) -> Result<(), CompilerDiagnostic> {
        let mut branches = Vec::new();
        let mut branch_selector = input.selector;
        let mut branch_context = input.then_context;
        let mut branch_location = input.location.clone();
        let mut branch_starts_after_else_if = false;
        let fallback;

        loop {
            let mut branch_template = empty_body_template_from(template);

            let boundary = self.parse_content(
                &branch_context,
                &mut branch_template,
                control_context.with_else_policy(ElseSentinelPolicy::SplitIf),
                InheritedChildWrapperPolicy::Skip,
            )?;

            if branch_starts_after_else_if {
                trim_leading_whitespace_atoms(&mut branch_template.content, self.string_table);
            }

            branches.push(TemplateConditionalBranch {
                selector: branch_selector,
                content: branch_template.content,
                render_plan: None,
                location: branch_location,
            });

            match boundary {
                TemplateBodyBoundary::ElseIf {
                    if_index,
                    close_index,
                    location,
                } => {
                    let parsed_else_if = self.parse_else_if_branch_header(
                        &input.else_context,
                        if_index,
                        close_index,
                        &location,
                    )?;
                    branch_selector = parsed_else_if.selector;
                    branch_context = parsed_else_if.branch_context;
                    branch_location = location;
                    branch_starts_after_else_if = true;
                }

                TemplateBodyBoundary::Else { location } => {
                    fallback = Some(self.parse_fallback_branch(
                        template,
                        &input.else_context,
                        control_context,
                        location,
                    )?);
                    break;
                }

                TemplateBodyBoundary::TemplateClose | TemplateBodyBoundary::Eof => {
                    fallback = None;
                    break;
                }
            }
        }

        template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(
            TemplateBranchChain {
                branches,
                fallback,
                location: input.location,
            },
        )));

        Ok(())
    }

    fn parse_fallback_branch(
        &mut self,
        owner: &Template,
        fallback_context: &ScopeContext,
        control_context: TemplateBodyControlContext,
        location: SourceLocation,
    ) -> Result<TemplateFallbackBranch, CompilerDiagnostic> {
        ensure_else_boundary_after_sentinel(self.token_stream, &location, self.string_table)?;

        let mut else_template = empty_body_template_from(owner);
        self.parse_content(
            fallback_context,
            &mut else_template,
            control_context.with_else_policy(ElseSentinelPolicy::Duplicate),
            InheritedChildWrapperPolicy::Skip,
        )?;

        ensure_else_content_starts_on_new_boundary(
            &else_template.content,
            &location,
            self.string_table,
        )?;
        trim_leading_whitespace_atoms(&mut else_template.content, self.string_table);

        Ok(TemplateFallbackBranch {
            content: else_template.content,
            render_plan: None,
            location,
        })
    }

    fn parse_else_if_branch_header(
        &mut self,
        base_context: &ScopeContext,
        if_index: usize,
        close_index: usize,
        location: &SourceLocation,
    ) -> Result<ParsedElseIfBranch, CompilerDiagnostic> {
        self.token_stream.index = if_index + 1;

        if next_meaningful_token_is_template_close(self.token_stream, close_index) {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::MissingTemplateElseIfCondition,
                location.clone(),
            ));
        }

        let parsed_header = parse_if_header(
            self.token_stream,
            base_context,
            self.type_interner,
            self.string_table,
        )?;

        if self.token_stream.index != close_index
            || !matches!(
                self.token_stream.current_token_kind(),
                TokenKind::TemplateClose
            )
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::MalformedTemplateElseIf,
                self.token_stream.current_location(),
            ));
        }

        self.token_stream.advance();
        ensure_else_boundary_after_sentinel(self.token_stream, location, self.string_table)
            .map_err(|diagnostic| remap_else_if_inline_diagnostic(diagnostic, location))?;

        let (mut selector, branch_context) =
            branch_selector_and_context_from_parsed_if_header(parsed_header, base_context, self)?;

        if self.control_flow_validation == TemplateControlFlowValidationMode::ConstRequired {
            selector = inline_source_consts_for_const_required_if_condition(
                selector,
                base_context,
                self.string_table,
            );
        }

        Ok(ParsedElseIfBranch {
            selector,
            branch_context,
        })
    }

    fn parse_loop_body(
        &mut self,
        template: &mut Template,
        input: TemplateLoopBodyParseInput,
        control_context: TemplateBodyControlContext,
    ) -> Result<(), CompilerDiagnostic> {
        let mut body_template = empty_body_template_from(template);

        self.parse_content(
            &input.body_context,
            &mut body_template,
            control_context.enter_template_loop(),
            InheritedChildWrapperPolicy::Skip,
        )?;

        template.control_flow = Some(TemplateControlFlow::Loop(Box::new(
            TemplateLoopControlFlow {
                header: input.header,
                body_content: body_template.content,
                body_render_plan: None,
                aggregate_render_plan: None,
                location: input.location,
            },
        )));

        Ok(())
    }

    /// Handles a nested `[...]` template token encountered inside a parent body.
    /// Recursively parses the child, then either folds it into the parent content
    /// or pushes it as a child template expression.
    fn parse_nested_template(
        &mut self,
        context: &ScopeContext,
        template: &mut Template,
        control_context: TemplateBodyControlContext,
        inherited_wrappers: InheritedChildWrapperPolicy,
    ) -> Result<(), CompilerDiagnostic> {
        let nested_inheritance = TemplateInheritance {
            direct_child_wrappers: template.style.child_templates.to_owned(),
        };

        let parse_options = NestedTemplateParseOptions {
            parsing_mode: if matches!(
                template.kind,
                TemplateType::Comment(CommentDirectiveKind::Doc)
            ) {
                TemplateParsingMode::DocComment
            } else {
                TemplateParsingMode::Standard
            },
            control_flow_validation: self.control_flow_validation,
            control_context,
        };

        let child_template = Template::new_nested_template(
            self.token_stream,
            context,
            self.type_interner,
            nested_inheritance,
            self.string_table,
            parse_options,
        )?;

        // Doc comment children are collected separately from template content.
        if matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) {
            template.doc_children.push(child_template);
            return Ok(());
        }

        if child_template.control_flow.is_some() {
            let expression = Expression::template(child_template, ValueMode::ImmutableOwned);
            template.content.add(expression);
            return Ok(());
        }

        match &child_template.kind {
            TemplateType::String
                if !child_template.has_unresolved_slots()
                    && !has_direct_child_template_outputs(&child_template) =>
            {
                ast_log!(
                    "Found a compile time foldable template inside a template. Folding into a string slice..."
                );

                let mut fold_context = context.new_template_fold_context(
                    self.string_table,
                    "nested compile-time template folding in body parser",
                )?;
                let folded_child_id = child_template
                    .fold_into_stringid(&mut fold_context)
                    .map_err(TemplateError::into_diagnostic)?;

                template.content.atoms.push(TemplateAtom::Content(
                    TemplateSegment::from_child_template_output(
                        Expression::string_slice(
                            folded_child_id,
                            self.token_stream.current_location(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                        child_template.clone_for_composition(),
                    ),
                ));

                return Ok(());
            }

            TemplateType::StringFunction => {
                *self.foldable = false;
            }

            TemplateType::Comment(_) => {
                return Ok(());
            }

            TemplateType::String | TemplateType::SlotInsert(_) => {}

            TemplateType::SlotDefinition(slot_key) => {
                let inherited_direct_child_wrappers = match inherited_wrappers {
                    InheritedChildWrapperPolicy::Apply => self.direct_child_wrappers.to_owned(),
                    InheritedChildWrapperPolicy::Skip => Vec::new(),
                };

                template.content.push_slot_with_wrappers(
                    slot_key.to_owned(),
                    inherited_direct_child_wrappers,
                    template.style.child_templates.to_owned(),
                    template.style.skip_parent_child_wrappers,
                );
                return Ok(());
            }
        }

        let expression = Expression::template(child_template, ValueMode::ImmutableOwned);
        template.content.add(expression);

        Ok(())
    }

    fn handle_loop_control_marker(
        &mut self,
        template: &mut Template,
        marker: &DirectLoopControlMarker,
        control_context: TemplateBodyControlContext,
    ) -> Result<(), CompilerDiagnostic> {
        if template.style.suppress_child_templates {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateLoopControlInLiteralBody,
                loop_control_marker_location(marker).clone(),
            ));
        }

        if !control_context.accepts_loop_control() {
            return Err(orphan_loop_control_diagnostic(marker));
        }

        let Some(close_index) = loop_control_marker_close_index(marker) else {
            return Err(malformed_loop_control_reason(marker));
        };

        ensure_loop_control_boundary_before_sentinel(self.token_stream, marker, self.string_table)?;
        trim_trailing_whitespace_atoms(&mut template.content, self.string_table);

        let mut control_template = empty_body_template_from(template);
        control_template.control_flow = Some(TemplateControlFlow::LoopControl(
            TemplateLoopControlSignal {
                kind: loop_control_kind(marker),
                location: loop_control_marker_location(marker).clone(),
            },
        ));

        template.content.add(Expression::template(
            control_template,
            ValueMode::ImmutableOwned,
        ));

        self.token_stream.index = close_index;
        self.token_stream.advance();
        ensure_loop_control_boundary_after_sentinel(self.token_stream, marker, self.string_table)
    }
}

fn loop_control_kind(marker: &DirectLoopControlMarker) -> TemplateLoopControlKind {
    match marker {
        DirectLoopControlMarker::Break { .. } => TemplateLoopControlKind::Break,
        DirectLoopControlMarker::Continue { .. } => TemplateLoopControlKind::Continue,
    }
}

struct ParsedElseIfBranch {
    selector: TemplateBranchSelector,
    branch_context: ScopeContext,
}

#[allow(clippy::result_large_err)]
fn branch_selector_and_context_from_parsed_if_header(
    parsed_header: ParsedIfHeader,
    base_context: &ScopeContext,
    parser: &mut TemplateBodyParser<'_, '_>,
) -> Result<(TemplateBranchSelector, ScopeContext), CompilerDiagnostic> {
    match parsed_header {
        ParsedIfHeader::BoolCondition { condition } => {
            let branch_context =
                base_context.new_child_control_flow(ContextKind::Branch, parser.string_table);

            Ok((TemplateBranchSelector::Bool(condition), branch_context))
        }

        ParsedIfHeader::OptionPresentCapture {
            scrutinee,
            pattern,
            then_context,
        } => {
            let branch_context =
                then_context.new_child_control_flow(ContextKind::Branch, parser.string_table);

            Ok((
                TemplateBranchSelector::OptionPresentCapture {
                    scrutinee,
                    pattern: Box::new(pattern),
                },
                branch_context,
            ))
        }

        ParsedIfHeader::MatchStyle { scrutinee } => {
            Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateMatchStyleControlFlowUnsupported,
                scrutinee.location,
            ))
        }
    }
}

fn next_meaningful_token_is_template_close(token_stream: &FileTokens, close_index: usize) -> bool {
    let mut index = token_stream.index;

    while index <= close_index && index < token_stream.length {
        match token_stream.tokens[index].kind {
            TokenKind::Newline => index += 1,
            TokenKind::TemplateClose => return true,
            _ => return false,
        }
    }

    true
}

#[derive(Clone, Copy)]
enum InheritedChildWrapperPolicy {
    // Normal template bodies apply wrappers inherited from their parent.
    Apply,
    // Control-flow branch bodies must not consume parent wrappers directly; the
    // composition pass attaches those wrappers to the control-flow child as a whole.
    Skip,
}

fn empty_body_template_from(owner: &Template) -> Template {
    let mut template = Template::empty();
    template.kind = owner.kind.to_owned();
    template.style = owner.style.to_owned();
    template.location = owner.location.to_owned();
    template
}

// -------------------------
//  Literal Content
// -------------------------

/// Consumes a `[...]` bracketed region as literal text when child templates are
/// suppressed (e.g. in `$doc` bodies). Tracks bracket nesting depth so balanced
/// brackets are included in the literal output.
fn consume_balanced_brackets_as_literal_text(
    token_stream: &mut FileTokens,
    template: &mut Template,
    string_table: &mut StringTable,
) {
    // Emit the opening bracket as literal text.
    let open_bracket_id = string_table.intern("[");
    template.content.add(Expression::string_slice(
        open_bracket_id,
        token_stream.current_location(),
        ValueMode::ImmutableOwned,
    ));
    token_stream.advance();

    let _ = consume_balanced_template_region(
        token_stream,
        |token, token_kind| match token_kind {
            TokenKind::TemplateHead => {
                let bracket_id = string_table.intern("[");
                template.content.add(Expression::string_slice(
                    bracket_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::TemplateClose => {
                let bracket_id = string_table.intern("]");
                template.content.add(Expression::string_slice(
                    bracket_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                template.content.add(Expression::string_slice(
                    *content,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::Newline => {
                let newline_id = string_table.intern("\n");
                template.content.add(Expression::string_slice(
                    newline_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::Symbol(id) | TokenKind::StyleDirective(id) => {
                let prefix = if matches!(token_kind, TokenKind::StyleDirective(_)) {
                    "$"
                } else {
                    ""
                };
                let name = string_table.resolve(*id).to_owned();
                let literal = format!("{prefix}{name}");
                let literal_id = string_table.intern(&literal);
                template.content.add(Expression::string_slice(
                    literal_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::StartTemplateBody | TokenKind::Colon => {
                let colon_id = string_table.intern(":");
                template.content.add(Expression::string_slice(
                    colon_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::Comma => {
                let comma_id = string_table.intern(",");
                template.content.add(Expression::string_slice(
                    comma_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::OpenParenthesis => {
                let paren_id = string_table.intern("(");
                template.content.add(Expression::string_slice(
                    paren_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::CloseParenthesis => {
                let paren_id = string_table.intern(")");
                template.content.add(Expression::string_slice(
                    paren_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            _ => {}
        },
        |_location| (),
    );
}

// -------------------------
//  Internal Helpers
// -------------------------

/// Returns true if the template contains any direct child template output atoms.
///
/// WHY:
/// - Folding such templates would merge those individual child outputs into one
///   string slice, losing the structure needed for `$children(..)` wrapper
///   application in slot composition.
fn has_direct_child_template_outputs(template: &Template) -> bool {
    template.content.atoms.iter().any(|atom| match atom {
        TemplateAtom::Content(segment) => segment.is_child_template_output,
        TemplateAtom::Slot(_) => false,
    })
}
