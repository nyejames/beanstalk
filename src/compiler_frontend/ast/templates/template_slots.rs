use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment,
    TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_composition::apply_inherited_child_templates_to_content;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_rule_error, return_syntax_error};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
struct SlotSchema {
    has_default_slot: bool,
    named_slots: FxHashSet<StringId>,
    positional_slots: BTreeSet<usize>,
}

impl SlotSchema {
    fn has_any_slots(&self) -> bool {
        self.has_default_slot || !self.named_slots.is_empty() || !self.positional_slots.is_empty()
    }

    fn accepts_target(&self, target: &SlotKey) -> bool {
        match target {
            SlotKey::Default => self.has_default_slot,
            SlotKey::Named(name) => self.named_slots.contains(name),
            SlotKey::Positional(index) => self.positional_slots.contains(index),
        }
    }

    fn ordered_positional_slots(&self) -> impl Iterator<Item = &usize> {
        self.positional_slots.iter()
    }
}

#[derive(Clone, Debug, Default)]
struct SlotContributions {
    default_atoms: Vec<TemplateAtom>,
    named_atoms: FxHashMap<StringId, Vec<TemplateAtom>>,
    positional_atoms: FxHashMap<usize, Vec<TemplateAtom>>,
}

impl SlotContributions {
    fn extend_named_atoms(&mut self, name: StringId, atoms: Vec<TemplateAtom>) {
        self.named_atoms.entry(name).or_default().extend(atoms);
    }

    fn extend_positional_atoms(&mut self, index: usize, atoms: Vec<TemplateAtom>) {
        self.positional_atoms
            .entry(index)
            .or_default()
            .extend(atoms);
    }

    fn atoms_for_slot(&self, key: &SlotKey) -> Vec<TemplateAtom> {
        match key {
            SlotKey::Default => self.default_atoms.clone(),
            SlotKey::Named(name) => self.named_atoms.get(name).cloned().unwrap_or_default(),
            SlotKey::Positional(index) => self
                .positional_atoms
                .get(index)
                .cloned()
                .unwrap_or_default(),
        }
    }
}

/// Composes a wrapper template by filling its slots with the provided content.
///
/// WHAT:
/// - Scans the wrapper for available slot targets (`Default`, `Named`, `Positional`).
/// - Partitions the authored `fill_content` into explicit `$insert` contributions and loose atoms.
/// - Routes loose atoms to positional slots first, then the default slot.
/// - Recursively replaces `SlotPlaceholder` atoms inside the wrapper with the matched contributions.
///
/// WHY:
/// - Connects the structural AST nodes generated during parsing into their final composed tree,
///   handling inheritance, ordering, and validation in one centralized pass.
pub(crate) fn compose_template_with_slots(
    wrapper: &Template,
    fill_content: &TemplateContent,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    let slot_schema = collect_slot_schema(wrapper, &location)?;
    if !slot_schema.has_any_slots() {
        return Err(CompilerError::compiler_error(
            "Internal template wrapper state error: expected at least one '$slot' while composing.",
        ));
    }

    let mut contributions = SlotContributions::default();
    let mut loose_atoms = Vec::new();

    // Walk authored fill content exactly once and bucket each atom either as:
    // 1) one or more explicit `$insert(...)` contributors, or
    // 2) loose content that should flow into positional or default slots.
    for atom in &fill_content.atoms {
        let (loose_atom, slot_inserts) = split_fill_atom_for_composition(atom);

        for (target, inserted_atoms) in slot_inserts {
            if !slot_schema.accepts_target(&target) {
                return unknown_slot_target_error(&target, &location);
            }

            match target {
                SlotKey::Default => {
                    contributions.default_atoms.extend(inserted_atoms);
                }
                SlotKey::Named(name) => {
                    contributions.extend_named_atoms(name, inserted_atoms);
                }
                SlotKey::Positional(index) => {
                    contributions.extend_positional_atoms(index, inserted_atoms);
                }
            }
        }

        if let Some(loose_atom) = loose_atom {
            loose_atoms.push(loose_atom);
        }
    }

    // Route loose content to positional slots first, then to the default slot.
    let loose_contributions = collect_loose_contributions(loose_atoms);
    let ordered_positional_slots: Vec<usize> =
        slot_schema.ordered_positional_slots().cloned().collect();

    for (contribution_index, contribution) in loose_contributions.into_iter().enumerate() {
        if let Some(slot_index) = ordered_positional_slots.get(contribution_index) {
            contributions.extend_positional_atoms(*slot_index, contribution.atoms);
            continue;
        }

        if slot_schema.has_default_slot {
            contributions.default_atoms.extend(contribution.atoms);
            continue;
        }

        if !slot_schema.positional_slots.is_empty() {
            return extra_loose_content_without_default_slot_error(&location);
        }

        return loose_content_without_default_slot_error(&location);
    }

    let atoms =
        compose_wrapper_atoms_recursive(&wrapper.content.atoms, &contributions, string_table)?;
    Ok(TemplateContent { atoms })
}

struct LooseContribution {
    atoms: Vec<TemplateAtom>,
}

fn collect_loose_contributions(atoms: Vec<TemplateAtom>) -> Vec<LooseContribution> {
    let mut contributions = Vec::new();
    let mut pending_loose_atoms = Vec::new();

    for atom in atoms {
        if is_top_level_template_contribution(&atom) {
            flush_pending_loose_atoms(&mut pending_loose_atoms, &mut contributions);
            contributions.push(LooseContribution { atoms: vec![atom] });
        } else {
            pending_loose_atoms.push(atom);
        }
    }

    flush_pending_loose_atoms(&mut pending_loose_atoms, &mut contributions);
    contributions
}

fn flush_pending_loose_atoms(
    pending: &mut Vec<TemplateAtom>,
    contributions: &mut Vec<LooseContribution>,
) {
    if pending.is_empty() {
        return;
    }

    // Coalesce contiguous loose atoms into a single contribution group
    contributions.push(LooseContribution {
        atoms: std::mem::take(pending),
    });
}

fn is_top_level_template_contribution(atom: &TemplateAtom) -> bool {
    // Folded child template outputs and explicit template expressions each represent
    // a separate positional contribution. Text that was authored inline (newlines,
    // whitespace, raw strings) is loose content that coalesces between contributions.
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    if segment.origin == TemplateSegmentOrigin::Head {
        return true;
    }

    // A child template that was folded into a string slice at parse time must still
    // be treated as its own contribution, so `$children(..)` wrappers are applied
    // to each child individually rather than to the merged text.
    if segment.is_child_template_output {
        return true;
    }

    matches!(segment.expression.kind, ExpressionKind::Template(_))
}

pub(crate) fn ensure_no_slot_insertions_remain(
    content: &TemplateContent,
    location: &SourceLocation,
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    if content.contains_slot_insertions() {
        return_rule_error!(
            "'$insert(...)' can only be used while filling an immediate parent template that defines matching '$slot' targets.",
            location.clone()
        );
    }

    Ok(())
}

fn collect_slot_schema(
    wrapper: &Template,
    error_location: &SourceLocation,
) -> Result<SlotSchema, CompilerError> {
    let mut schema = SlotSchema::default();
    collect_slot_schema_atoms(&wrapper.content.atoms, &mut schema, error_location)?;
    Ok(schema)
}

fn collect_slot_schema_atoms(
    atoms: &[TemplateAtom],
    schema: &mut SlotSchema,
    error_location: &SourceLocation,
) -> Result<(), CompilerError> {
    // This recursive walk intentionally traverses nested template expressions so a
    // wrapper template can declare slots at any depth while still being resolved in
    // one deterministic pass.
    for atom in atoms {
        match atom {
            TemplateAtom::Slot(slot) => match &slot.key {
                SlotKey::Default => {
                    if schema.has_default_slot {
                        return_rule_error!(
                            "Templates can only define one default '$slot'.",
                            error_location.to_owned()
                        );
                    }
                    schema.has_default_slot = true;
                }
                SlotKey::Named(name) => {
                    schema.named_slots.insert(*name);
                }
                SlotKey::Positional(index) => {
                    schema.positional_slots.insert(*index);
                }
            },
            TemplateAtom::Content(segment) => {
                if let ExpressionKind::Template(template) = &segment.expression.kind {
                    collect_slot_schema_atoms(&template.content.atoms, schema, error_location)?;
                }
            }
        }
    }

    Ok(())
}

pub fn parse_slot_definition_target_argument(
    token_stream: &mut FileTokens,
    _string_table: &StringTable,
) -> Result<SlotKey, CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(SlotKey::Default);
    }

    // Move from `StyleDirective("slot")` to the directive argument
    // and leave the parser positioned at `)` on success.
    token_stream.advance();
    token_stream.advance();

    let target = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => SlotKey::Named(*name),
        TokenKind::IntLiteral(index) => {
            if *index <= 0 {
                return_syntax_error!(
                    format!(
                        "'$slot({})' is invalid. Positional slots start at 1.",
                        index
                    ),
                    token_stream.current_location()
                );
            }
            SlotKey::Positional(*index as usize)
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$slot()' cannot use empty parentheses. Use '$slot' for default, a quoted name like '$slot(\"style\")', or a positive integer like '$slot(1)'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$slot(...)' only accepts a quoted string literal name or a positive integer.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Expected ')' after template slot directive argument.",
            token_stream.current_location(),
            {
                SuggestedInsertion => ")",
            }
        );
    }

    Ok(target)
}

pub fn parse_required_named_slot_insert_argument(
    token_stream: &mut FileTokens,
    _string_table: &StringTable,
) -> Result<StringId, CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_syntax_error!(
            "'$insert' requires a quoted named target like '$insert(\"style\")'.",
            token_stream.current_location()
        );
    }

    token_stream.advance();
    token_stream.advance();

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::IntLiteral(_) => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$insert()' cannot use empty parentheses. Use quoted names like '$insert(\"style\")'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Expected ')' after template insert directive argument.",
            token_stream.current_location(),
            {
                SuggestedInsertion => ")",
            }
        );
    }

    Ok(slot_name)
}

fn compose_wrapper_atoms_recursive(
    wrapper_atoms: &[TemplateAtom],
    contributions: &SlotContributions,
    string_table: &StringTable,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut composed = Vec::with_capacity(wrapper_atoms.len());

    for atom in wrapper_atoms {
        match atom {
            TemplateAtom::Slot(slot) => {
                // Slot replacement is intentionally non-consuming, so duplicate named
                // slot declarations replay the same aggregated contribution in each place.
                composed.extend(expand_slot_placeholder(slot, contributions, string_table)?);
            }
            TemplateAtom::Content(segment) => {
                // Nested templates can carry slot definitions too. Recursively resolve
                // them with the same contribution buckets so authored hierarchy is kept.
                if let ExpressionKind::Template(template) = &segment.expression.kind
                    && template.has_unresolved_slots()
                {
                    let mut nested_template = template.as_ref().to_owned();
                    nested_template.content = TemplateContent {
                        atoms: compose_wrapper_atoms_recursive(
                            &nested_template.content.atoms,
                            contributions,
                            string_table,
                        )?,
                    };
                    nested_template.unformatted_content = nested_template.content.to_owned();
                    nested_template.content_needs_formatting = false;
                    nested_template.render_plan = None;

                    let mut nested_expression = segment.expression.to_owned();
                    nested_expression.kind = ExpressionKind::Template(Box::new(nested_template));
                    composed.push(TemplateAtom::Content(TemplateSegment::new(
                        nested_expression,
                        segment.origin,
                    )));
                    continue;
                }

                composed.push(atom.to_owned());
            }
        }
    }

    Ok(composed)
}

fn expand_slot_placeholder(
    slot: &SlotPlaceholder,
    contributions: &SlotContributions,
    string_table: &StringTable,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let slot_atoms = contributions.atoms_for_slot(&slot.key);
    if slot.applied_child_wrappers.is_empty() && slot.child_wrappers.is_empty() {
        return Ok(slot_atoms);
    }

    let mut expanded = Vec::with_capacity(slot_atoms.len());
    for atom in slot_atoms {
        let atom = if slot.child_wrappers.is_empty() {
            atom
        } else if contribution_has_direct_child_templates(&atom) {
            // Contribution is a template with direct child templates inside — apply
            // the child wrappers to those inner children.
            apply_child_wrappers_to_contribution_children(
                &atom,
                &slot.child_wrappers,
                string_table,
            )?
        } else if contribution_is_child_template_output(&atom)
            || contribution_template(&atom).is_some()
        {
            // Contribution is either a folded child template string slice or an
            // unfolded template — wrap the whole contribution in the child wrapper.
            wrap_child_slot_contribution(&atom, &slot.child_wrappers, string_table)?
        } else {
            atom
        };

        if !slot.skip_parent_child_wrappers
            && !slot.applied_child_wrappers.is_empty()
            && is_child_slot_contribution(&atom)
        {
            expanded.push(wrap_child_slot_contribution(
                &atom,
                &slot.applied_child_wrappers,
                string_table,
            )?);
        } else {
            expanded.push(atom);
        }
    }

    Ok(expanded)
}

fn is_child_slot_contribution(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    segment.is_child_template_output
        || matches!(segment.expression.kind, ExpressionKind::Template(_))
}

fn contribution_has_direct_child_templates(atom: &TemplateAtom) -> bool {
    let Some(template) = contribution_template(atom) else {
        return false;
    };

    template
        .content
        .atoms
        .iter()
        .any(is_direct_child_template_atom)
}

fn is_direct_child_template_atom(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    if segment.origin != TemplateSegmentOrigin::Body {
        return false;
    }

    if segment.is_child_template_output {
        return true;
    }

    match &segment.expression.kind {
        ExpressionKind::Template(template) => !template.has_unresolved_slots(),
        _ => false,
    }
}

fn wrap_child_slot_contribution(
    atom: &TemplateAtom,
    child_wrappers: &[Template],
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let wrapped_content = apply_inherited_child_templates_to_content(
        TemplateContent {
            atoms: vec![atom.to_owned()],
        },
        child_wrappers,
        string_table,
    )?;

    let origin = contribution_origin(atom);
    let mut wrapped_template = Template::create_default(vec![]);
    wrapped_template.content = wrapped_content;
    wrapped_template.unformatted_content = wrapped_template.content.to_owned();
    refresh_template_kind(&mut wrapped_template);
    wrapped_template.location = contribution_location(atom);

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(wrapped_template, Ownership::ImmutableOwned),
        origin,
    )))
}

fn apply_child_wrappers_to_contribution_children(
    atom: &TemplateAtom,
    child_wrappers: &[Template],
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let Some(mut contribution_template) = contribution_template(atom) else {
        return Ok(atom.to_owned());
    };

    contribution_template.content = apply_inherited_child_templates_to_content(
        contribution_template.content,
        child_wrappers,
        string_table,
    )?;
    contribution_template.unformatted_content = contribution_template.content.to_owned();
    contribution_template.content_needs_formatting = false;
    // Clear the stale render plan so fold_into_stringid rebuilds it from the
    // now-modified content (which includes the freshly-applied child wrappers).
    contribution_template.render_plan = None;
    refresh_template_kind(&mut contribution_template);

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(contribution_template, Ownership::ImmutableOwned),
        contribution_origin(atom),
    )))
}

fn contribution_template(atom: &TemplateAtom) -> Option<Template> {
    let TemplateAtom::Content(segment) = atom else {
        return None;
    };

    if let Some(source_child_template) = &segment.source_child_template {
        return Some(source_child_template.as_ref().to_owned());
    }

    match &segment.expression.kind {
        ExpressionKind::Template(template) => Some(template.as_ref().to_owned()),
        _ => None,
    }
}

/// Returns true when the atom is a child template output that was folded into a
/// string slice at parse time. These must be treated like template contributions
/// for the purpose of applying `$children(..)` wrappers in slot expansion.
fn contribution_is_child_template_output(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };
    segment.is_child_template_output
}

fn refresh_template_kind(template: &mut Template) {
    if matches!(
        template.kind,
        TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_) | TemplateType::Comment(_)
    ) {
        return;
    }

    template.kind = if template.content.is_const_evaluable_value()
        && !template.content.contains_slot_insertions()
    {
        TemplateType::String
    } else {
        TemplateType::StringFunction
    };
}

fn contribution_origin(atom: &TemplateAtom) -> TemplateSegmentOrigin {
    match atom {
        TemplateAtom::Content(segment) => segment.origin,
        TemplateAtom::Slot(_) => TemplateSegmentOrigin::Body,
    }
}

fn contribution_location(atom: &TemplateAtom) -> SourceLocation {
    match atom {
        TemplateAtom::Content(segment) => segment.expression.location.to_owned(),
        TemplateAtom::Slot(_) => SourceLocation::default(),
    }
}

fn slot_insert_from_atom(atom: &TemplateAtom) -> Option<(SlotKey, &TemplateContent)> {
    match atom {
        TemplateAtom::Slot(_) => None,
        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => match &template.kind {
                TemplateType::SlotInsert(target) => Some((target.to_owned(), &template.content)),
                _ => None,
            },
            _ => None,
        },
    }
}

fn split_fill_atom_for_composition(
    atom: &TemplateAtom,
) -> (Option<TemplateAtom>, Vec<(SlotKey, Vec<TemplateAtom>)>) {
    let Some((target, slot_insert_content)) = slot_insert_from_atom(atom) else {
        let TemplateAtom::Content(segment) = atom else {
            return (Some(atom.to_owned()), Vec::new());
        };

        let ExpressionKind::Template(template) = &segment.expression.kind else {
            return (Some(atom.to_owned()), Vec::new());
        };

        let (sanitized_template, extracted_inserts) =
            collect_direct_slot_insert_contributions(template);
        if extracted_inserts.is_empty() {
            return (Some(atom.to_owned()), extracted_inserts);
        }

        if sanitized_template.content.is_empty() {
            return (None, extracted_inserts);
        }

        let mut sanitized_expression = segment.expression.to_owned();
        sanitized_expression.kind = ExpressionKind::Template(Box::new(sanitized_template));
        return (
            Some(TemplateAtom::Content(TemplateSegment::new(
                sanitized_expression,
                segment.origin,
            ))),
            extracted_inserts,
        );
    };

    (None, vec![(target, slot_insert_content.atoms.clone())])
}

fn collect_direct_slot_insert_contributions(
    template: &Template,
) -> (Template, Vec<(SlotKey, Vec<TemplateAtom>)>) {
    let mut sanitized_atoms = Vec::with_capacity(template.content.atoms.len());
    let mut extracted = Vec::new();

    // Only direct child `$insert(...)` helpers are extracted here. Nested descendants
    // are left untouched so they cannot bypass immediate-parent slot scoping.
    for atom in &template.content.atoms {
        if let Some((target, slot_insert_content)) = slot_insert_from_atom(atom) {
            extracted.push((target, slot_insert_content.atoms.clone()));
            continue;
        }

        sanitized_atoms.push(atom.to_owned());
    }

    if extracted.is_empty() {
        return (template.to_owned(), extracted);
    }

    let mut sanitized = template.to_owned();
    sanitized.content = TemplateContent {
        atoms: sanitized_atoms,
    };

    (sanitized, extracted)
}

fn extra_loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        "This template defines positional '$slot(n)' targets but no default '$slot'. There is more loose content than positional slots available.",
        location.to_owned()
    );
}

fn loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        "This template defines named '$slot(...)' targets without a default '$slot'. Loose content is not allowed here; use '$insert(\"name\")'.",
        location.to_owned()
    );
}

fn unknown_slot_target_error(
    target: &SlotKey,
    location: &SourceLocation,
) -> Result<TemplateContent, CompilerError> {
    match target {
        SlotKey::Default => {
            return_rule_error!(
                "'$insert' cannot target the default slot because the parent template does not define '$slot'.",
                location.to_owned()
            )
        }
        SlotKey::Named(_) => {
            return_rule_error!(
                "'$insert(\"name\")' targets a named slot that does not exist on the immediate parent template.",
                location.to_owned()
            )
        }
        SlotKey::Positional(index) => {
            return_rule_error!(
                format!(
                    "'$insert' targets positional slot '{}' which does not exist on the immediate parent template.",
                    index
                ),
                location.to_owned()
            )
        }
    }
}

#[cfg(test)]
#[path = "tests/slots_tests.rs"]
mod slots_tests;
