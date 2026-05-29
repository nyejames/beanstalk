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
use crate::compiler_frontend::ast::templates::template_render_units::prepare_conditional_child_wrapper_render_plan;
use crate::compiler_frontend::ast::templates::template_slots::{
    SlotResolutionMode, SlotResolutionOutcome, TemplateSlotError, resolve_slot_application,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

use std::sync::Arc;

// -------------------------
//  Child Wrapper Application
// -------------------------

/// Applies `$children(..)` wrapper templates to direct child template atoms
/// in the content. Non-child atoms are passed through unchanged.
pub(in crate::compiler_frontend::ast::templates) fn apply_inherited_child_templates_to_content(
    content: TemplateContent,
    inherited_templates: &[Template],
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<TemplateContent, TemplateSlotError> {
    if inherited_templates.is_empty() {
        return Ok(content);
    }

    let mut wrapped_atoms = Vec::with_capacity(content.atoms.len());

    for atom in content.atoms {
        if let Some(control_flow_atom) =
            attach_conditional_child_wrappers(&atom, inherited_templates, string_table)?
        {
            wrapped_atoms.push(control_flow_atom);
            continue;
        }

        if atom.is_direct_child_template_atom() {
            wrapped_atoms.push(wrap_direct_child_atom(
                &atom,
                inherited_templates,
                string_table,
                resolution_mode,
            )?);
        } else {
            wrapped_atoms.push(atom);
        }
    }

    Ok(TemplateContent {
        atoms: wrapped_atoms,
    })
}

/// Attaches parent `$children(..)` wrappers to control-flow children without
/// externally wrapping the child expression.
///
/// A template `if` or `loop` may emit no output. Wrapping the child as soon as
/// the parent sees it would render wrappers on skipped branches or empty loops.
/// Storing the inherited wrappers on the child lets later folding/lowering apply
/// them only after the child structurally emits output.
fn attach_conditional_child_wrappers(
    atom: &TemplateAtom,
    inherited_templates: &[Template],
    string_table: &StringTable,
) -> Result<Option<TemplateAtom>, TemplateSlotError> {
    let TemplateAtom::Content(segment) = atom else {
        return Ok(None);
    };

    if segment.origin != TemplateSegmentOrigin::Body {
        return Ok(None);
    }

    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return Ok(None);
    };

    if !template.is_control_flow_template() {
        return Ok(None);
    }

    if template.style.skip_parent_child_wrappers || inherited_templates.is_empty() {
        return Ok(Some(atom.to_owned()));
    }

    let mut child_template = template.as_ref().clone_for_composition();
    child_template
        .conditional_child_wrappers
        .extend(inherited_templates.iter().cloned());
    child_template.conditional_child_wrapper_plan =
        Some(prepare_conditional_child_wrapper_render_plan(
            &child_template.conditional_child_wrappers,
            string_table,
        )?);

    let mut expression = segment.expression.to_owned();
    expression.kind = ExpressionKind::Template(Box::new(child_template));

    let mut segment = segment.to_owned();
    segment.expression = expression;

    Ok(Some(TemplateAtom::Content(segment)))
}

/// Wraps a direct child atom in all inherited wrapper templates (applied
/// outermost-first by iterating in reverse).
pub(in crate::compiler_frontend::ast::templates) fn wrap_direct_child_atom(
    atom: &TemplateAtom,
    inherited_templates: &[Template],
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<TemplateAtom, TemplateSlotError> {
    let mut wrapped_atom = atom.to_owned();

    for wrapper in inherited_templates.iter().rev() {
        wrapped_atom =
            wrap_atom_in_child_template(&wrapped_atom, wrapper, string_table, resolution_mode)?;
    }

    Ok(wrapped_atom)
}

/// Wraps a single atom inside a child wrapper template.
///
/// If the wrapper has unresolved slots, the atom is composed into those slots.
/// Otherwise, the wrapper content is prepended so the child atom follows the wrapper.
fn wrap_atom_in_child_template(
    atom: &TemplateAtom,
    wrapper: &Template,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<TemplateAtom, TemplateSlotError> {
    let origin = match atom {
        TemplateAtom::Content(segment) => segment.origin,
        TemplateAtom::Slot(_) => TemplateSegmentOrigin::Body,
    };

    let wrapped_template = if wrapper.has_unresolved_slots() {
        let fill_content = TemplateContent {
            atoms: vec![atom.to_owned()],
        };

        match resolve_slot_application(
            wrapper,
            fill_content,
            &wrapper.location,
            string_table,
            resolution_mode,
        )? {
            SlotResolutionOutcome::Composed(composed_content) => {
                let mut wrapped_template = wrapper.clone_for_composition();
                wrapped_template.content = composed_content;
                wrapped_template.resync_composition_metadata();
                wrapped_template
            }
            SlotResolutionOutcome::Runtime(runtime_plan) => {
                let mut wrapped_template = Template::empty();
                wrapped_template.runtime_slot_application = Some(runtime_plan);
                wrapped_template.location = wrapper.location.to_owned();
                wrapped_template.resync_runtime_metadata();
                wrapped_template
            }
        }
    } else {
        let mut wrapped_template = Template::empty();
        wrapped_template.location = wrapper.location.to_owned();

        wrapped_template.content = TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(
                        wrapper.clone_for_composition(),
                        ValueMode::ImmutableOwned,
                    ),
                    TemplateSegmentOrigin::Body,
                )),
                atom.to_owned(),
            ],
        };

        wrapped_template.resync_composition_metadata();
        wrapped_template
    };

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(wrapped_template, ValueMode::ImmutableOwned),
        origin,
    )))
}

// -------------------------
//  Head-Chain Composition
// -------------------------

/// Stores authored template atoms once while pending head-chain items refer
/// to them by index.
///
/// WHY: pending composition items are routing metadata, not the final owned
/// atom tree. Keeping atoms in a pool makes the intermediate graph smaller
/// and clearer.
#[derive(Clone, Debug, Default)]
struct PendingAtomPool {
    atoms: Vec<Option<TemplateAtom>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PendingAtomId(usize);

impl PendingAtomPool {
    fn push(&mut self, atom: TemplateAtom) -> PendingAtomId {
        let id = PendingAtomId(self.atoms.len());
        self.atoms.push(Some(atom));
        id
    }

    fn take(&mut self, id: PendingAtomId) -> Result<TemplateAtom, TemplateSlotError> {
        let Some(slot) = self.atoms.get_mut(id.0) else {
            return Err(CompilerError::compiler_error(
                "Template head-chain composition referenced an unknown pending atom.",
            )
            .into());
        };

        match slot.take() {
            Some(atom) => Ok(atom),
            None => Err(CompilerError::compiler_error(
                "Template head-chain composition consumed a pending atom more than once.",
            )
            .into()),
        }
    }
}

/// Items in a pending head-chain composition. Each item is either a reference
/// to a pooled atom or a reference to a chain layer (wrapper template with
/// fill content).
#[derive(Clone, Debug)]
enum PendingChainItem {
    AtomRef(PendingAtomId),
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
pub(in crate::compiler_frontend::ast::templates) fn compose_template_head_chain(
    content: &TemplateContent,
    foldable: &mut bool,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<TemplateContent, TemplateSlotError> {
    // Cheap pre-scan: if no head atoms exist, composition is a no-op.
    let has_head_atoms = content.atoms.iter().any(is_head_content_atom);
    if !has_head_atoms {
        return Ok(content.to_owned());
    }

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

    // Second cheap check: if head atoms exist but none can open a receiving layer,
    // skip the full chain machinery and return the content unchanged.
    let has_receiver = head_atoms
        .iter()
        .any(|atom| receiver_template_from_head_atom(atom).is_some());
    if !has_receiver {
        return Ok(content.to_owned());
    }

    let mut root_items = Vec::new();
    let mut layers = Vec::new();
    let mut active_layer: Option<usize> = None;
    let mut atom_pool = PendingAtomPool::default();

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
                wrapper: receiver.clone_for_composition(),
                fill_items: Vec::new(),
            });
            active_layer = Some(layer_index);
            continue;
        }

        push_pending_atom(
            &mut root_items,
            &mut layers,
            active_layer,
            &mut atom_pool,
            atom,
        );
    }

    // Body atoms are appended after head parsing. If the head opened a receiving
    // chain, body atoms become contributions to the deepest active receiver.
    for atom in body_atoms {
        push_pending_atom(
            &mut root_items,
            &mut layers,
            active_layer,
            &mut atom_pool,
            atom,
        );
    }

    let mut cache = rustc_hash::FxHashMap::default();
    let resolved_atoms = resolve_pending_chain_items(
        &root_items,
        &layers,
        &mut atom_pool,
        &mut cache,
        string_table,
        resolution_mode,
    )?;

    Ok(TemplateContent {
        atoms: resolved_atoms,
    })
}

// -------------------------
//  Chain Resolution Helpers
// -------------------------

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

/// Stores an atom in the pool and routes an `AtomRef` item to the root list
/// or the active receiving layer.
fn push_pending_atom(
    root_items: &mut Vec<PendingChainItem>,
    layers: &mut [ChainLayer],
    active_layer: Option<usize>,
    atom_pool: &mut PendingAtomPool,
    atom: TemplateAtom,
) {
    let item = PendingChainItem::AtomRef(atom_pool.push(atom));
    push_chain_item(root_items, layers, active_layer, item);
}

/// Returns true if the atom is a head-origin content segment.
fn is_head_content_atom(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };
    segment.origin == TemplateSegmentOrigin::Head
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
    atom_pool: &mut PendingAtomPool,
    cache: &mut rustc_hash::FxHashMap<usize, Arc<Template>>,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<Vec<TemplateAtom>, TemplateSlotError> {
    let mut resolved_atoms = Vec::with_capacity(items.len());

    for item in items {
        match item {
            PendingChainItem::AtomRef(atom_id) => resolved_atoms.push(atom_pool.take(*atom_id)?),

            PendingChainItem::LayerRef {
                layer_index,
                origin,
            } => {
                let resolved_layer = resolve_chain_layer(
                    *layer_index,
                    layers,
                    atom_pool,
                    cache,
                    string_table,
                    resolution_mode,
                )?;

                resolved_atoms.push(TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(
                        resolved_layer.as_ref().clone_for_composition(),
                        ValueMode::ImmutableOwned,
                    ),
                    *origin,
                )));
            }
        }
    }

    Ok(resolved_atoms)
}

/// Resolves a single chain layer by filling its wrapper's slots with the
/// accumulated fill items. Caches resolved layers to avoid redundant work.
fn resolve_chain_layer(
    layer_index: usize,
    layers: &[ChainLayer],
    atom_pool: &mut PendingAtomPool,
    cache: &mut rustc_hash::FxHashMap<usize, Arc<Template>>,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<Arc<Template>, TemplateSlotError> {
    if let Some(cached) = cache.get(&layer_index) {
        return Ok(Arc::clone(cached));
    }

    let layer = &layers[layer_index];

    if layer.fill_items.is_empty() {
        // Head-only wrapper references like `[format.table]` must stay as unresolved
        // wrapper templates so later use-sites can still fill their slots.
        // This is expected for reusable template/style constants and should not
        // be treated as an escaped helper artifact.
        let wrapper = Arc::new(layer.wrapper.clone_for_composition());
        cache.insert(layer_index, Arc::clone(&wrapper));
        return Ok(wrapper);
    }

    let fill_atoms = resolve_pending_chain_items(
        &layer.fill_items,
        layers,
        atom_pool,
        cache,
        string_table,
        resolution_mode,
    )?;

    let resolved_fill = TemplateContent { atoms: fill_atoms };

    let outcome = resolve_slot_application(
        &layer.wrapper,
        resolved_fill,
        &layer.wrapper.location,
        string_table,
        resolution_mode,
    )?;

    let resolved_wrapper = match outcome {
        SlotResolutionOutcome::Composed(composed_content) => {
            let mut resolved_wrapper = layer.wrapper.clone_for_composition();
            resolved_wrapper.content = composed_content;
            resolved_wrapper.resync_composition_metadata();
            resolved_wrapper
        }
        SlotResolutionOutcome::Runtime(runtime_plan) => {
            let mut resolved_wrapper = Template::empty();
            resolved_wrapper.runtime_slot_application = Some(runtime_plan);
            resolved_wrapper.location = layer.wrapper.location.clone();
            resolved_wrapper.resync_runtime_metadata();
            resolved_wrapper
        }
    };

    let resolved_wrapper = Arc::new(resolved_wrapper);
    cache.insert(layer_index, Arc::clone(&resolved_wrapper));

    Ok(resolved_wrapper)
}
