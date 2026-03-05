use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegment, TemplateType,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::return_rule_error;

#[derive(Clone, Debug)]
struct SlotFillState {
    gap_buckets: Vec<Vec<TemplateAtom>>,
    slot_buckets: Vec<Vec<TemplateAtom>>,
    slot_used: Vec<bool>,
    expected_next_slot: usize,
    current_gap_index: usize,
    saw_explicit_slot_directive: bool,
}

impl SlotFillState {
    fn new(slot_count: usize) -> Self {
        Self {
            gap_buckets: vec![Vec::new(); slot_count + 1],
            slot_buckets: vec![Vec::new(); slot_count],
            slot_used: vec![false; slot_count],
            expected_next_slot: 1,
            current_gap_index: 0,
            saw_explicit_slot_directive: false,
        }
    }

    fn slot_count(&self) -> usize {
        self.slot_buckets.len()
    }
}

// Slot application is kept in one module so the template parser can stay focused on
// token-to-node parsing, while this file owns the wrapper-filling state machine.
pub(crate) fn compose_template_with_slots(
    wrapper: &Template,
    fill_content: &TemplateContent,
    error_location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    let slot_count = wrapper.content.total_slot_count();
    if slot_count == 0 {
        return Err(CompilerError::compiler_error(
            "Internal template wrapper state error: expected at least one slot while composing.",
        ));
    }

    let mut state = SlotFillState::new(slot_count);

    for atom in &fill_content.atoms {
        let Some(slot_index) = slot_target_from_atom(atom) else {
            state.gap_buckets[state.current_gap_index].push(atom.clone());
            continue;
        };

        if slot_index == 0 || slot_index > state.slot_count() {
            return slot_out_of_range_error(slot_index, state.slot_count(), error_location);
        }

        if state.slot_used[slot_index - 1] {
            return duplicate_slot_error(slot_index, error_location);
        }

        if state.slot_count() > 1 && slot_index != state.expected_next_slot {
            return out_of_order_slot_error(slot_index, state.expected_next_slot, error_location);
        }

        state.slot_used[slot_index - 1] = true;
        state.saw_explicit_slot_directive = true;
        state.current_gap_index = slot_index;
        state.expected_next_slot = slot_index + 1;

        let slot_content = slot_content_from_atom(atom).expect("checked above");
        state.slot_buckets[slot_index - 1].extend(slot_content.atoms.clone());
    }

    if state.slot_count() == 1 && !state.saw_explicit_slot_directive {
        state.slot_buckets[0] = std::mem::take(&mut state.gap_buckets[0]);
    }

    if state.slot_count() > 1 && state.slot_used.iter().any(|used| !used) {
        return missing_slot_error(state.slot_count(), error_location);
    }

    let mut slot_cursor = 0usize;
    let mut atoms =
        compose_wrapper_atoms_recursive(&wrapper.content.atoms, &mut state, &mut slot_cursor)?;
    if slot_cursor != state.slot_count() {
        return Err(CompilerError::compiler_error(
            "Internal slot composition mismatch: not all wrapper slots were consumed.",
        ));
    }

    atoms.extend(state.gap_buckets[state.slot_count()].clone());
    Ok(TemplateContent { atoms })
}

pub(crate) fn ensure_no_slot_insertions_remain(
    content: &TemplateContent,
    location: &TextLocation,
) -> Result<(), CompilerError> {
    if content.contains_slot_insertions() {
        return_rule_error!(
            "Labeled slot insertions can only be used while filling a template that defines slots.",
            location.to_owned().to_error_location_without_table()
        );
    }

    Ok(())
}

fn compose_wrapper_atoms_recursive(
    wrapper_atoms: &[TemplateAtom],
    state: &mut SlotFillState,
    slot_cursor: &mut usize,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut composed = Vec::with_capacity(wrapper_atoms.len());

    for atom in wrapper_atoms {
        match atom {
            TemplateAtom::Slot => {
                if *slot_cursor >= state.slot_count() {
                    return Err(CompilerError::compiler_error(
                        "Internal slot composition mismatch: resolved more slots than expected.",
                    ));
                }

                let slot_index = *slot_cursor;
                composed.extend(state.gap_buckets[slot_index].clone());
                composed.extend(state.slot_buckets[slot_index].clone());
                *slot_cursor += 1;
            }
            TemplateAtom::Content(segment) => match &segment.expression.kind {
                ExpressionKind::Template(template) if template.has_unresolved_slots() => {
                    let mut nested_template = template.as_ref().to_owned();
                    nested_template.content = TemplateContent {
                        atoms: compose_wrapper_atoms_recursive(
                            &nested_template.content.atoms,
                            state,
                            slot_cursor,
                        )?,
                    };

                    let mut nested_expression = segment.expression.to_owned();
                    nested_expression.kind = ExpressionKind::Template(Box::new(nested_template));
                    composed.push(TemplateAtom::Content(TemplateSegment::new(
                        nested_expression,
                        segment.origin,
                    )));
                }
                _ => composed.push(atom.to_owned()),
            },
        }
    }

    Ok(composed)
}

fn slot_target_from_atom(atom: &TemplateAtom) -> Option<usize> {
    match atom {
        TemplateAtom::Slot => None,
        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => match template.kind {
                TemplateType::SlotInsertion(slot) => Some(slot),
                _ => None,
            },
            _ => None,
        },
    }
}

fn slot_content_from_atom(atom: &TemplateAtom) -> Option<&TemplateContent> {
    match atom {
        TemplateAtom::Slot => None,
        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => match template.kind {
                TemplateType::SlotInsertion(_) => Some(&template.content),
                _ => None,
            },
            _ => None,
        },
    }
}

fn slot_out_of_range_error(
    slot_index: usize,
    slot_count: usize,
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        format!("Slot ${slot_index} is out of range. This template defines {slot_count} slot(s)."),
        location.to_owned().to_error_location_without_table()
    );
}

fn duplicate_slot_error(
    slot_index: usize,
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        format!("Slot ${slot_index} is used more than once."),
        location.to_owned().to_error_location_without_table()
    );
}

fn out_of_order_slot_error(
    slot_index: usize,
    expected_slot: usize,
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        format!(
            "Slot ${slot_index} is out of order. Labeled slots must be used in ascending order, expected '${expected_slot}' next."
        ),
        location.to_owned().to_error_location_without_table()
    );
}

fn missing_slot_error(
    slot_count: usize,
    location: &TextLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        format!(
            "All {slot_count} slots must be used exactly once. Use '[$1: first slot content]' for content and '[$2]' to leave a slot empty."
        ),
        location.to_owned().to_error_location_without_table()
    );
}
