//! Template composition: head-chain resolution, child wrapper application,
//! and style inheritance.
//!
//! WHAT: Applies Beanstalk's template semantics after parsing — head-chain
//! composition and `$children(..)` wrapper application.
//!
//! WHY: Keeps composition separate from token-level parsing so each phase
//! has a clear input/output contract.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_slots::compose_template_with_slots;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::string_interning::StringTable;

// -------------------------
// CHILD WRAPPER APPLICATION
// -------------------------

/// Applies `$children(..)` wrapper templates to direct child template atoms
/// in the content. Non-child atoms are passed through unchanged.
pub(crate) fn apply_inherited_child_templates_to_content(
    content: TemplateContent,
    inherited_templates: &[Template],
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    if inherited_templates.is_empty() {
        return Ok(content);
    }

    let mut wrapped_atoms = Vec::with_capacity(content.atoms.len());

    for atom in content.atoms {
        if is_direct_child_template_atom(&atom) {
            wrapped_atoms.push(wrap_direct_child_atom(
                &atom,
                inherited_templates,
                string_table,
            )?);
        } else {
            wrapped_atoms.push(atom);
        }
    }

    Ok(TemplateContent {
        atoms: wrapped_atoms,
    })
}

/// Returns true if the atom is a direct child template in body position
/// (either a folded child output or an unresolved template expression).
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

/// Wraps a direct child atom in all inherited wrapper templates (applied
/// outermost-first by iterating in reverse).
fn wrap_direct_child_atom(
    atom: &TemplateAtom,
    inherited_templates: &[Template],
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let mut wrapped_atom = atom.to_owned();

    for wrapper in inherited_templates.iter().rev() {
        wrapped_atom = wrap_atom_in_child_template(&wrapped_atom, wrapper, string_table)?;
    }

    Ok(wrapped_atom)
}

/// Wraps a single atom inside a child wrapper template. If the wrapper has
/// slots, the atom is composed into those slots. Otherwise, the wrapper
/// content is prepended.
fn wrap_atom_in_child_template(
    atom: &TemplateAtom,
    wrapper: &Template,
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let origin = match atom {
        TemplateAtom::Content(segment) => segment.origin,
        TemplateAtom::Slot(_) => TemplateSegmentOrigin::Body,
    };

    let wrapped_template = if wrapper.has_unresolved_slots() {
        let fill_content = TemplateContent {
            atoms: vec![atom.to_owned()],
        };
        let composed_content =
            compose_template_with_slots(wrapper, &fill_content, &wrapper.location, string_table)?;

        let mut wrapped_template = wrapper.to_owned();
        wrapped_template.content = composed_content;
        wrapped_template.unformatted_content = wrapped_template.content.to_owned();
        wrapped_template.content_needs_formatting = false;
        wrapped_template.render_plan = None;
        wrapped_template
    } else {
        let mut wrapped_template = Template::create_default(vec![]);
        wrapped_template.location = wrapper.location.to_owned();
        wrapped_template.content = TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(wrapper.to_owned(), Ownership::ImmutableOwned),
                    TemplateSegmentOrigin::Body,
                )),
                atom.to_owned(),
            ],
        };
        wrapped_template.unformatted_content = wrapped_template.content.to_owned();
        wrapped_template.kind = if wrapped_template.content.is_const_evaluable_value()
            && !wrapped_template.content.contains_slot_insertions()
        {
            TemplateType::String
        } else {
            TemplateType::StringFunction
        };
        wrapped_template
    };

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(wrapped_template, Ownership::ImmutableOwned),
        origin,
    )))
}

// -------------------------
// HEAD-CHAIN COMPOSITION
// -------------------------

/// Items in a pending head-chain composition. Each item is either a literal
/// atom or a reference to a chain layer (wrapper template with fill content).
#[derive(Clone, Debug)]
enum PendingChainItem {
    Atom(TemplateAtom),
    LayerRef {
        layer_index: usize,
        origin: TemplateSegmentOrigin,
    },
}

/// A single layer in the head-chain: a wrapper template and the items that
/// should fill its slots.
#[derive(Clone, Debug)]
struct ChainLayer {
    wrapper: Template,
    fill_items: Vec<PendingChainItem>,
}

/// Resolves head-chain composition for a template's content.
///
/// Head atoms that are wrapper templates (with unresolved slots) open new
/// receiving layers. Subsequent head/body atoms become fill content for the
/// deepest active layer. The chain is then resolved bottom-up.
pub(crate) fn compose_template_head_chain(
    content: &TemplateContent,
    foldable: &mut bool,
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    let mut head_atoms = Vec::new();
    let mut body_atoms = Vec::new();

    // Keep head and body atoms separated so only head template arguments can open
    // new receiving layers. Body atoms still flow into the deepest active receiver.
    for atom in &content.atoms {
        match atom {
            TemplateAtom::Content(segment) if segment.origin == TemplateSegmentOrigin::Head => {
                head_atoms.push(atom.to_owned());
            }
            _ => body_atoms.push(atom.to_owned()),
        }
    }

    if head_atoms.is_empty() {
        return Ok(content.to_owned());
    }

    let mut root_items = Vec::new();
    let mut layers = Vec::new();
    let mut active_layer: Option<usize> = None;

    for atom in head_atoms {
        if let Some((receiver, origin)) = receiver_template_from_head_atom(&atom) {
            let layer_index = layers.len();

            push_chain_item(
                &mut root_items,
                &mut layers,
                active_layer,
                PendingChainItem::LayerRef {
                    layer_index,
                    origin,
                },
            );

            if matches!(receiver.kind, TemplateType::StringFunction) {
                *foldable = false;
            }

            layers.push(ChainLayer {
                wrapper: receiver.to_owned(),
                fill_items: Vec::new(),
            });
            active_layer = Some(layer_index);
            continue;
        }

        push_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            PendingChainItem::Atom(atom),
        );
    }

    // Body atoms are appended after head parsing. If the head opened a receiving
    // chain, body atoms become contributions to the deepest active receiver.
    for atom in body_atoms {
        push_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            PendingChainItem::Atom(atom),
        );
    }

    let mut cache = rustc_hash::FxHashMap::default();
    let atoms = resolve_pending_chain_items(&root_items, &layers, &mut cache, string_table)?;
    Ok(TemplateContent { atoms })
}

/// Routes a chain item to either the root list or the active receiving layer.
fn push_chain_item(
    root_items: &mut Vec<PendingChainItem>,
    layers: &mut [ChainLayer],
    active_layer: Option<usize>,
    item: PendingChainItem,
) {
    match active_layer {
        Some(layer_index) => layers[layer_index].fill_items.push(item),
        None => root_items.push(item),
    }
}

/// Checks if a head atom is a wrapper template (has unresolved slots) that
/// should open a new receiving chain layer.
fn receiver_template_from_head_atom(
    atom: &TemplateAtom,
) -> Option<(&Template, TemplateSegmentOrigin)> {
    let TemplateAtom::Content(segment) = atom else {
        return None;
    };

    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return None;
    };

    if !template.has_unresolved_slots() {
        return None;
    }

    if matches!(
        template.kind,
        TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_)
    ) {
        return None;
    }

    Some((template, segment.origin))
}

/// Recursively resolves pending chain items into concrete template atoms.
fn resolve_pending_chain_items(
    items: &[PendingChainItem],
    layers: &[ChainLayer],
    cache: &mut rustc_hash::FxHashMap<usize, Template>,
    string_table: &StringTable,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut atoms = Vec::with_capacity(items.len());

    for item in items {
        match item {
            PendingChainItem::Atom(atom) => atoms.push(atom.to_owned()),
            PendingChainItem::LayerRef {
                layer_index,
                origin,
            } => {
                let resolved_layer =
                    resolve_chain_layer(*layer_index, layers, cache, string_table)?;
                atoms.push(TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(resolved_layer, Ownership::ImmutableOwned),
                    *origin,
                )));
            }
        }
    }

    Ok(atoms)
}

/// Resolves a single chain layer by filling its wrapper's slots with the
/// accumulated fill items. Caches resolved layers to avoid redundant work.
fn resolve_chain_layer(
    layer_index: usize,
    layers: &[ChainLayer],
    cache: &mut rustc_hash::FxHashMap<usize, Template>,
    string_table: &StringTable,
) -> Result<Template, CompilerError> {
    if let Some(cached) = cache.get(&layer_index) {
        return Ok(cached.to_owned());
    }

    let layer = &layers[layer_index];
    if layer.fill_items.is_empty() {
        // Head-only wrapper references like `[format.table]` must stay as unresolved
        // wrapper templates so later use-sites can still fill their slots.
        cache.insert(layer_index, layer.wrapper.to_owned());
        return Ok(layer.wrapper.to_owned());
    }

    let resolved_fill_atoms =
        resolve_pending_chain_items(&layer.fill_items, layers, cache, string_table)?;
    let resolved_fill = TemplateContent {
        atoms: resolved_fill_atoms,
    };
    let composed_content = compose_template_with_slots(
        &layer.wrapper,
        &resolved_fill,
        &layer.wrapper.location,
        string_table,
    )?;

    let mut resolved_wrapper = layer.wrapper.to_owned();
    resolved_wrapper.content = composed_content;
    resolved_wrapper.unformatted_content = resolved_wrapper.content.to_owned();
    resolved_wrapper.content_needs_formatting = false;
    resolved_wrapper.render_plan = None;
    cache.insert(layer_index, resolved_wrapper.to_owned());

    Ok(resolved_wrapper)
}
