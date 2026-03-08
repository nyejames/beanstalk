use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateAtom, TemplateContent, TemplateSegment, TemplateType,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::{return_rule_error, return_syntax_error};
use rustc_hash::{FxHashMap, FxHashSet};
use crate::compiler_frontend::string_interning::{StringId, StringTable};

#[derive(Clone, Debug, Default)]
struct SlotSchema {
    has_default_slot: bool,
    named_slots: FxHashSet<crate::compiler_frontend::string_interning::StringId>,
}

impl SlotSchema {
    fn has_any_slots(&self) -> bool {
        self.has_default_slot || !self.named_slots.is_empty()
    }

    fn accepts_target(&self, target: &SlotKey) -> bool {
        match target {
            SlotKey::Default => self.has_default_slot,
            SlotKey::Named(name) => self.named_slots.contains(name),
        }
    }

    fn has_named_slots_without_default(&self) -> bool {
        !self.has_default_slot && !self.named_slots.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
struct SlotContributions {
    default_atoms: Vec<TemplateAtom>,
    named_atoms: FxHashMap<crate::compiler_frontend::string_interning::StringId, Vec<TemplateAtom>>,
}

impl SlotContributions {
    fn add_default_atom(&mut self, atom: TemplateAtom) {
        self.default_atoms.push(atom);
    }

    fn extend_named_atoms(
        &mut self,
        name: crate::compiler_frontend::string_interning::StringId,
        atoms: Vec<TemplateAtom>,
    ) {
        self.named_atoms.entry(name).or_default().extend(atoms);
    }

    fn atoms_for_slot(&self, key: &SlotKey) -> Vec<TemplateAtom> {
        match key {
            SlotKey::Default => self.default_atoms.clone(),
            SlotKey::Named(name) => self.named_atoms.get(name).cloned().unwrap_or_default(),
        }
    }
}

// Slot application is kept in one module so the template parser can stay focused on
// token-to-node parsing, while this file owns the wrapper-filling state machine.
pub(crate) fn compose_template_with_slots(
    wrapper: &Template,
    fill_content: &TemplateContent,
    error_location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    let slot_schema = collect_slot_schema(wrapper, error_location)?;
    if !slot_schema.has_any_slots() {
        return Err(CompilerError::compiler_error(
            "Internal template wrapper state error: expected at least one '$slot' while composing.",
        ));
    }

    let mut contributions = SlotContributions::default();

    // Walk authored fill content exactly once and bucket each atom either as:
    // 1) one or more explicit `$insert(...)` contributors, or
    // 2) loose content that should flow into the default slot.
    //
    // A fill atom can legally do both at once (template has `$slot` + `$insert`):
    // - it renders as loose/default content for this wrapper
    // - and it contributes named content upward to this wrapper.
    for atom in &fill_content.atoms {
        let (loose_atom, slot_inserts) = split_fill_atom_for_composition(atom);

        for (target, inserted_atoms) in slot_inserts {
            if !slot_schema.accepts_target(&target) {
                return unknown_slot_target_error(&target, error_location);
            }

            match target {
                SlotKey::Default => {
                    contributions.default_atoms.extend(inserted_atoms);
                }
                SlotKey::Named(name) => {
                    contributions.extend_named_atoms(name, inserted_atoms);
                }
            }
        }

        if let Some(loose_atom) = loose_atom {
            if slot_schema.has_named_slots_without_default() {
                return loose_content_without_default_slot_error(error_location);
            }

            contributions.add_default_atom(loose_atom);
        }
    }

    let atoms = compose_wrapper_atoms_recursive(&wrapper.content.atoms, &contributions)?;
    Ok(TemplateContent { atoms })
}

pub(crate) fn ensure_no_slot_insertions_remain(
    content: &TemplateContent,
    location: &TextLocation,
) -> Result<(), CompilerError> {
    if content.contains_slot_insertions() {
        return_rule_error!(
            "'$insert(...)' can only be used while filling an immediate parent template that defines matching '$slot' targets.",
            location.to_owned().to_error_location_without_table()
        );
    }

    Ok(())
}

fn collect_slot_schema(
    wrapper: &Template,
    error_location: &TextLocation,
) -> Result<SlotSchema, CompilerError> {
    let mut schema = SlotSchema::default();
    collect_slot_schema_atoms(&wrapper.content.atoms, &mut schema, error_location)?;
    Ok(schema)
}

fn collect_slot_schema_atoms(
    atoms: &[TemplateAtom],
    schema: &mut SlotSchema,
    error_location: &TextLocation,
) -> Result<(), CompilerError> {
    // This recursive walk intentionally traverses nested template expressions so a
    // wrapper template can declare slots at any depth while still being resolved in
    // one deterministic pass.
    for atom in atoms {
        match atom {
            TemplateAtom::Slot(SlotKey::Default) => {
                if schema.has_default_slot {
                    return_rule_error!(
                        "Templates can only define one default '$slot'.",
                        error_location.to_owned().to_error_location_without_table()
                    );
                }
                schema.has_default_slot = true;
            }
            TemplateAtom::Slot(SlotKey::Named(name)) => {
                schema.named_slots.insert(*name);
            }
            TemplateAtom::Content(segment) => {
                if let ExpressionKind::Template(template) = &segment.expression.kind {
                    collect_slot_schema_atoms(&template.content.atoms, schema, error_location)?;
                }
            }
        }
    }

    Ok(())
}


pub fn parse_optional_slot_name_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<Option<StringId>, CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(None);
    }

    // Move from `StyleDirective("slot")`/`StyleDirective("insert")` to the
    // directive argument and leave the parser positioned at `)` on success.
    token_stream.advance();
    token_stream.advance();

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$slot()' and '$insert()' cannot use empty parentheses. Use '$slot' for default or quoted names like '$slot(\"style\")'.",
                token_stream
                    .current_location()
                    .to_error_location(string_table)
            );
        }
        _ => {
            return_syntax_error!(
                "'$slot(...)' and '$insert(...)' only accept quoted string literal names.",
                token_stream
                    .current_location()
                    .to_error_location(string_table)
            );
        }
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Expected ')' after template slot directive argument.",
            token_stream.current_location().to_error_location(string_table),
            {
                SuggestedInsertion => ")",
            }
        );
    }

    Ok(Some(slot_name))
}

pub fn parse_required_slot_name_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<StringId, CompilerError> {
    let slot_name = parse_optional_slot_name_argument(token_stream, string_table)?;
    let Some(slot_name) = slot_name else {
        return_syntax_error!(
            "'$insert' requires a quoted named target like '$insert(\"style\")'.",
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    };

    Ok(slot_name)
}

fn compose_wrapper_atoms_recursive(
    wrapper_atoms: &[TemplateAtom],
    contributions: &SlotContributions,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut composed = Vec::with_capacity(wrapper_atoms.len());

    for atom in wrapper_atoms {
        match atom {
            TemplateAtom::Slot(key) => {
                // Slot replacement is intentionally non-consuming so duplicate named
                // slot declarations replay the same aggregated contribution in each place.
                composed.extend(contributions.atoms_for_slot(key));
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
                        )?,
                    };

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

fn loose_content_without_default_slot_error(
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        "This template defines named '$slot(...)' targets without a default '$slot'. Loose content is not allowed here; use '$insert(\\\"name\\\")'.",
        location.to_owned().to_error_location_without_table()
    );
}

fn unknown_slot_target_error(
    target: &SlotKey,
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    match target {
        SlotKey::Default => {
            return_rule_error!(
                "'$insert' cannot target the default slot because the parent template does not define '$slot'.",
                location.to_owned().to_error_location_without_table()
            )
        }
        SlotKey::Named(_) => {
            return_rule_error!(
                "'$insert(\\\"name\\\")' targets a named slot that does not exist on the immediate parent template.",
                location.to_owned().to_error_location_without_table()
            )
        }
    }
}
