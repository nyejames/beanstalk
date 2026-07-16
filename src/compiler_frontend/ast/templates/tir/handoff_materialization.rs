//! Owned runtime-template handoff materialization from TIR.
//!
//! WHAT: builds an owned, recursive runtime-template tree from `TemplateIrStore`
//! for the AST-to-HIR boundary.
//!
//! WHY: HIR should consume finalized runtime template metadata without holding
//! raw `TemplateIrId`, `TemplateIrNodeId`, or `TemplateSlotPlanId` values. This
//! module keeps those IDs internal to AST/TIR traversal and returns the neutral
//! owned handoff shapes defined in `runtime_handoff.rs` that HIR lowering
//! consumes directly.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::runtime_handoff::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeSlotContributionSource, OwnedRuntimeSlotSite,
    OwnedRuntimeSlotSiteRenderPiece, OwnedRuntimeSlotSiteRenderPlan, OwnedRuntimeTemplateBody,
    OwnedRuntimeTemplateBranch, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};

use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySetId, TirSlotResolutionKind, TirWrapperApplicationMode, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::slot_composition::collect_tir_slot_schema;
use crate::compiler_frontend::ast::templates::tir::slot_plan::{
    TemplateSlotPlan, TemplateSlotSiteRenderPiece,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::tir::{
    fold_tir_view, tir_view_is_empty_overlay_linear_fold_safe,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

impl TemplateIrStore {
    /// Test-only entry point that materializes a runtime template directly by ID.
    ///
    /// WHAT: creates a materializer without a fold context or registry and
    ///       delegates to the inner required handoff owner.
    /// WHY: production finalization always goes through the view-backed fold
    ///      context variant. This entry point exists only for focused tests that
    ///      need to exercise materialization without constructing a full view.
    #[cfg(test)]
    pub(crate) fn owned_runtime_template_handoff_for_template(
        &self,
        id: TemplateIrId,
    ) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
        let mut materializer = RuntimeHandoffMaterializer::new(self);
        materializer.owned_runtime_template_handoff_for_template(id)
    }

    /// Materializes a runtime slot application from the caller's finalized
    /// effective view.
    ///
    /// WHAT: uses the `TirView` root and expression overlays while preserving
    /// the existing owned slot handoff shape.
    /// WHY: AST finalization must replace runtime templates from the same
    /// effective view it used for runtime classification, without leaking TIR
    /// IDs past the AST/HIR boundary.
    pub(crate) fn owned_runtime_slot_handoff_for_tir_view(
        &self,
        view: &TirView<'_>,
        registry: Rc<RefCell<TemplateIrRegistry>>,
    ) -> Result<Option<OwnedRuntimeSlotApplicationHandoff>, CompilerError> {
        let template_id = self.same_store_template_id_for_view(view)?;
        Self::validate_registry_for_view(view, &registry)?;

        let mut materializer = RuntimeHandoffMaterializer::new_with_registry_and_overlay(
            self,
            registry,
            view.overlay_set_id(),
        );
        materializer.owned_runtime_slot_handoff_for_template(template_id)
    }

    /// Materializes an ordinary runtime template from the caller's finalized
    /// effective view while retaining the existing child-fold shortcut.
    pub(crate) fn owned_runtime_template_handoff_for_tir_view_with_fold_context(
        &self,
        view: &TirView<'_>,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
        let template_id = self.same_store_template_id_for_view(view)?;
        let registry = fold_context
            .template_ir_registry
            .as_ref()
            .map(Rc::clone)
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR HIR handoff view-backed fold-context materialization requires a registry, but the fold context has none.",
                )
            })?;
        Self::validate_registry_for_view(view, &registry)?;

        let mut materializer =
            RuntimeHandoffMaterializer::new_with_fold_context(self, fold_context, registry);
        materializer.overlay_set_stack.push(view.overlay_set_id());
        materializer.owned_runtime_template_handoff_for_template(template_id)
    }

    fn validate_registry_for_view(
        view: &TirView<'_>,
        registry: &Rc<RefCell<TemplateIrRegistry>>,
    ) -> Result<(), CompilerError> {
        let registry = registry.borrow();
        if !std::ptr::eq(view.registry_ref(), &*registry) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff materialization registry does not own the supplied view.",
            ));
        }

        Ok(())
    }

    fn same_store_template_id_for_view(
        &self,
        view: &TirView<'_>,
    ) -> Result<TemplateIrId, CompilerError> {
        if view.root_ref().store_id != self.store_id() {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff view materialization view store does not match the supplied store.",
            ));
        }

        let view_store = view.store()?;
        let view_store_owner = view_store.owner();
        let supplied_store_owner = self.owner();
        if !Arc::ptr_eq(&view_store_owner, &supplied_store_owner) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff view materialization registered store does not match the supplied store.",
            ));
        }

        let template_id = view.root_ref().template_id;
        let store_root = self
            .get_template(template_id)
            .map(|template| template.root)
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR HIR handoff view materialization referenced a missing template.",
                )
            })?;
        let registry_root = view.root_template()?.root;

        if registry_root != store_root {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff view materialization root does not match the supplied store.",
            ));
        }

        Ok(template_id)
    }
}

struct RuntimeHandoffMaterializer<'store, 'context, 'fold> {
    store: &'store TemplateIrStore,
    registry: Option<Rc<RefCell<TemplateIrRegistry>>>,
    fold_context: Option<&'context mut TemplateFoldContext<'fold>>,
    /// Stack of overlay-set IDs for the templates currently being materialized.
    ///
    /// WHAT: the top entry is the overlay set that applies to the current
    ///       subtree. The top-level template view pushes its overlay set first;
    ///       each nested child template temporarily pushes its own overlay set.
    /// WHY: one finalized root overlay covers every expression site reachable
    ///      within a template, while child templates retain separate effective
    ///      identities.
    overlay_set_stack: Vec<TemplateOverlaySetId>,
    template_stack: Vec<TemplateRef>,
    node_stack: Vec<TemplateIrNodeId>,
}

impl<'store> RuntimeHandoffMaterializer<'store, 'static, 'static> {
    #[cfg(test)]
    fn new(store: &'store TemplateIrStore) -> Self {
        Self {
            store,
            registry: None,
            fold_context: None,
            overlay_set_stack: Vec::new(),
            template_stack: Vec::new(),
            node_stack: Vec::new(),
        }
    }
}

impl<'store, 'context, 'fold> RuntimeHandoffMaterializer<'store, 'context, 'fold> {
    fn new_with_fold_context(
        store: &'store TemplateIrStore,
        fold_context: &'context mut TemplateFoldContext<'fold>,
        registry: Rc<RefCell<TemplateIrRegistry>>,
    ) -> Self {
        Self {
            store,
            registry: Some(registry),
            fold_context: Some(fold_context),
            overlay_set_stack: Vec::new(),
            template_stack: Vec::new(),
            node_stack: Vec::new(),
        }
    }

    fn new_with_registry_and_overlay(
        store: &'store TemplateIrStore,
        registry: Rc<RefCell<TemplateIrRegistry>>,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Self {
        Self {
            store,
            registry: Some(registry),
            fold_context: None,
            overlay_set_stack: vec![overlay_set_id],
            template_stack: Vec::new(),
            node_stack: Vec::new(),
        }
    }

    /// Creates a nested materializer scoped to a foreign store, inheriting the
    /// parent's qualified template stack for cross-store cycle detection.
    ///
    /// WHAT: borrows the foreign store, starts with the given overlay set as the
    ///       body-root overlay, and seeds `template_stack` with the ancestor
    ///       `TemplateRef`s so A-store -> B-store -> A-store cycles are caught
    ///       across store boundaries. The `node_stack` starts fresh because node
    ///       IDs are store-local and cannot alias across stores.
    /// WHY: both cross-store child and wrapper materialization need a materializer
    ///      scoped to the owning store while preserving ancestor recursion.
    ///      Sharing this constructor keeps the nested-materializer shape in one
    ///      place instead of duplicating struct literals at each call site.
    fn nested_foreign_store_materializer<'foreign>(
        &self,
        foreign_store: &'foreign TemplateIrStore,
        overlay_set_id: Option<TemplateOverlaySetId>,
    ) -> Result<RuntimeHandoffMaterializer<'foreign, 'static, 'static>, CompilerError> {
        if let Some(overlay_set_id) = overlay_set_id {
            self.validate_overlay_set(overlay_set_id)?;
        }
        let registry = self.registry.as_ref().map(Rc::clone);
        Ok(RuntimeHandoffMaterializer {
            store: foreign_store,
            registry,
            fold_context: None,
            overlay_set_stack: overlay_set_id.into_iter().collect(),
            template_stack: self.template_stack.clone(),
            node_stack: Vec::new(),
        })
    }

    /// Validates one registry-owned overlay set before it becomes active.
    ///
    /// WHAT: uses the registry's canonical overlay-set lookup and permits the
    ///       empty ID only for the direct test materializer that has no
    ///       registry.
    /// WHY: wrapper and child references carry exact overlay identity. A
    ///      missing set must fail before traversal rather than being mistaken
    ///      for an empty context.
    fn validate_overlay_set(
        &self,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Result<bool, CompilerError> {
        let Some(registry) = self.registry.as_ref().map(Rc::clone) else {
            if overlay_set_id != TemplateOverlaySetId::empty() {
                return Err(CompilerError::compiler_error(
                    "TIR HIR handoff materialization requires a registry for a non-empty overlay set.",
                ));
            }

            return Ok(false);
        };

        let registry = registry.borrow();
        if registry.overlay_set(overlay_set_id).is_none() {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff materialization referenced missing overlay set {}",
                overlay_set_id
            )));
        }

        Ok(true)
    }

    /// Temporarily activates one exact overlay set while materializing a
    /// nested root, restoring the parent stack even when traversal fails.
    fn with_overlay_set<T>(
        &mut self,
        overlay_set_id: TemplateOverlaySetId,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        let should_push = self.validate_overlay_set(overlay_set_id)?;
        if !should_push {
            return build(self);
        }

        self.overlay_set_stack.push(overlay_set_id);
        let result = build(self);
        let popped_overlay_set = self.overlay_set_stack.pop();
        debug_assert_eq!(popped_overlay_set, Some(overlay_set_id));
        result
    }

    fn owned_runtime_slot_handoff_for_template(
        &mut self,
        id: TemplateIrId,
    ) -> Result<Option<OwnedRuntimeSlotApplicationHandoff>, CompilerError> {
        let template = self.get_template(id)?;
        let root = template.root;
        let Some(slot_plan_id) = template.runtime_slot_plan else {
            return Ok(None);
        };

        self.with_template_on_stack(
            TemplateRef::new(self.store.store_id(), id),
            |materializer| {
                materializer.materialize_runtime_slot_application_by_parts(root, slot_plan_id, None)
            },
        )
        .map(Some)
    }

    fn owned_runtime_template_handoff_for_template(
        &mut self,
        id: TemplateIrId,
    ) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
        // `materialize_template` already validates the template exists and
        // pushes it onto the recursion stack so child-template cycles are
        // detected there.
        self.materialize_template(id, None, None)
    }

    fn materialize_template(
        &mut self,
        id: TemplateIrId,
        active_slot_plan: Option<TemplateSlotPlanId>,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
        let template = self.get_template(id)?;
        let location = template.location.clone();
        let runtime_slot_plan = template.runtime_slot_plan;
        let root = template.root;

        self.with_template_on_stack(
            TemplateRef::new(self.store.store_id(), id),
            |materializer| {
                let body = if let Some(slot_plan_id) = runtime_slot_plan {
                    OwnedRuntimeTemplateBody::RuntimeSlotApplication(Box::new(
                        materializer.materialize_runtime_slot_application_by_parts(
                            root,
                            slot_plan_id,
                            injection,
                        )?,
                    ))
                } else {
                    OwnedRuntimeTemplateBody::Render(materializer.materialize_node_with_injection(
                        root,
                        active_slot_plan,
                        injection,
                    )?)
                };

                Ok(OwnedRuntimeTemplateHandoff { body, location })
            },
        )
    }

    fn materialize_runtime_slot_application_by_parts(
        &mut self,
        wrapper_root: TemplateIrNodeId,
        slot_plan_id: TemplateSlotPlanId,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeSlotApplicationHandoff, CompilerError> {
        let slot_plan = self.get_slot_plan(slot_plan_id)?.clone();
        let wrapper =
            self.materialize_node_with_injection(wrapper_root, Some(slot_plan_id), injection)?;
        let contribution_sources =
            self.materialize_contribution_sources(&slot_plan, slot_plan_id)?;
        let slot_sites = self.materialize_slot_sites(&slot_plan, slot_plan_id)?;

        Ok(OwnedRuntimeSlotApplicationHandoff {
            wrapper,
            contribution_sources,
            slot_sites,
            location: slot_plan.location,
        })
    }

    fn materialize_contribution_sources(
        &mut self,
        slot_plan: &TemplateSlotPlan,
        slot_plan_id: TemplateSlotPlanId,
    ) -> Result<Vec<OwnedRuntimeSlotContributionSource>, CompilerError> {
        let mut sources = Vec::with_capacity(slot_plan.contribution_sources.len());

        for source in &slot_plan.contribution_sources {
            sources.push(OwnedRuntimeSlotContributionSource {
                source: source.source,
                render_root: self.materialize_node(source.render_root, Some(slot_plan_id))?,
                renders_wrapper_unconditionally: source.renders_wrapper_unconditionally,
                location: source.location.clone(),
            });
        }

        Ok(sources)
    }

    fn materialize_slot_sites(
        &mut self,
        slot_plan: &TemplateSlotPlan,
        slot_plan_id: TemplateSlotPlanId,
    ) -> Result<Vec<OwnedRuntimeSlotSite>, CompilerError> {
        let mut sites = Vec::with_capacity(slot_plan.slot_sites.len());

        for site in &slot_plan.slot_sites {
            let mut pieces = Vec::with_capacity(site.render_plan.pieces.len());
            for piece in &site.render_plan.pieces {
                pieces.push(match piece {
                    TemplateSlotSiteRenderPiece::Render(node_id) => {
                        OwnedRuntimeSlotSiteRenderPiece::Render(
                            self.materialize_node(*node_id, Some(slot_plan_id))?,
                        )
                    }

                    TemplateSlotSiteRenderPiece::ContributionSource(source_id) => {
                        OwnedRuntimeSlotSiteRenderPiece::ContributionSource(*source_id)
                    }
                });
            }

            sites.push(OwnedRuntimeSlotSite {
                site: site.site,
                render_plan: OwnedRuntimeSlotSiteRenderPlan { pieces },
                location: site.location.clone(),
            });
        }

        Ok(sites)
    }

    fn materialize_node(
        &mut self,
        id: TemplateIrNodeId,
        active_slot_plan: Option<TemplateSlotPlanId>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        self.materialize_node_with_injection(id, active_slot_plan, None)
    }

    /// Materializes one TIR node through the canonical handoff walker, with an
    /// optional inherited child injected at matching slot placeholders.
    ///
    /// WHAT: keeps ordinary node materialization and wrapper fill injection on
    ///       the same structural traversal, including branches, loops and
    ///       same-store child-template roots.
    /// WHY: wrapper target selection is schema-owned, so the handoff walker must
    ///      be able to replace every structural shape that schema discovery can
    ///      reach without creating a second, partial materializer.
    fn materialize_node_with_injection(
        &mut self,
        id: TemplateIrNodeId,
        active_slot_plan: Option<TemplateSlotPlanId>,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let node = self.effective_node(id)?;

        let owned_node = self.with_node_on_stack(id, |materializer| {
            match node.kind {
                TemplateIrNodeKind::Sequence { children } => {
                    let mut owned_children = Vec::with_capacity(children.len());
                    for child in children {
                        owned_children.push(materializer.materialize_node_with_injection(
                            child,
                            active_slot_plan,
                            injection,
                        )?);
                    }

                    Ok(OwnedRuntimeTemplateNode::Sequence {
                        children: owned_children,
                    })
                }

                TemplateIrNodeKind::Text {
                    text,
                    byte_len,
                    origin: _,
                } => Ok(OwnedRuntimeTemplateNode::Text {
                    text,
                    byte_len,
                    reactive_subscription: self.store.node_reactive_subscription(id).cloned(),
                    location: node.location,
                }),

                TemplateIrNodeKind::DynamicExpression {
                    expression,
                    origin: _,
                    reactive_subscription,
                    site_id,
                } => Ok(OwnedRuntimeTemplateNode::DynamicExpression {
                    expression: Box::new(
                        materializer.effective_expression(site_id, expression.as_ref())?,
                    ),
                    reactive_subscription,
                }),

                TemplateIrNodeKind::ChildTemplate {
                    reference,
                    occurrence_id,
                } => {
                    let wrapper_context = materializer
                        .effective_wrapper_context_for_occurrence(occurrence_id)?;
                    let child_handoff = materializer.materialize_child_template_node(
                        &reference,
                        &node.location,
                        active_slot_plan,
                        injection,
                    )?;

                    if let Some(context) = wrapper_context {
                        materializer.apply_wrapper_context_overlay_to_child_handoff(
                            &context,
                            child_handoff,
                            &node.location,
                        )
                    } else {
                        Ok(child_handoff)
                    }
                }

                TemplateIrNodeKind::BranchChain { branches, fallback } => {
                    let mut owned_branches = Vec::with_capacity(branches.len());
                    for branch in branches {
                        let body = materializer.materialize_node_with_injection(
                            branch.body,
                            active_slot_plan,
                            injection,
                        )?;

                        owned_branches.push(OwnedRuntimeTemplateBranch {
                            selector: materializer.effective_branch_selector(
                                &branch.selector,
                                branch.selector_site_id,
                            )?,
                            body,
                            location: branch.location,
                        });
                    }

                    let fallback = if let Some(fallback_id) = fallback {
                        Some(Box::new(
                            materializer.materialize_node_with_injection(
                                fallback_id,
                                active_slot_plan,
                                injection,
                            )?,
                        ))
                    } else {
                        None
                    };

                    Ok(OwnedRuntimeTemplateNode::BranchChain {
                        branches: owned_branches,
                        fallback,
                        location: node.location,
                    })
                }

                TemplateIrNodeKind::Loop {
                    header,
                    header_sites,
                    body,
                    aggregate_wrapper,
                    ..
                } => {
                    let body_node = materializer.materialize_node_with_injection(
                        body,
                        active_slot_plan,
                        injection,
                    )?;

                    let aggregate_wrapper = if let Some(wrapper_id) = aggregate_wrapper {
                        Some(Box::new(
                            materializer.materialize_node_with_injection(
                                wrapper_id,
                                active_slot_plan,
                                injection,
                            )?,
                        ))
                    } else {
                        None
                    };

                    Ok(OwnedRuntimeTemplateNode::Loop {
                        header: materializer.effective_loop_header(
                            &header,
                            header_sites,
                        )?,
                        body: Box::new(body_node),
                        aggregate_wrapper,
                        location: node.location,
                    })
                }

                TemplateIrNodeKind::AggregateOutput => {
                    Ok(OwnedRuntimeTemplateNode::AggregateOutput)
                }

                TemplateIrNodeKind::LoopControl { kind } => {
                    Ok(OwnedRuntimeTemplateNode::LoopControl {
                        kind,
                        location: node.location,
                    })
                }

                TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
                    if Some(plan) != active_slot_plan {
                        return Err(CompilerError::compiler_error(
                            "TIR HIR handoff materialization found a runtime slot site outside its owning slot application.",
                        ));
                    }

                    Ok(OwnedRuntimeTemplateNode::RuntimeSlotSite { site })
                }

                TemplateIrNodeKind::Slot { placeholder } => {
                    if let Some((fill_target_key, child_handoff)) = injection
                        && placeholder.key == *fill_target_key
                    {
                        return Ok(child_handoff.clone());
                    }

                    if let Some(resolution) = materializer
                        .effective_slot_resolution_for_occurrence(placeholder.occurrence_id)?
                        && let TirSlotResolutionKind::Resolved { sources } = resolution.kind
                    {
                        return materializer.materialize_resolved_slot_sources(
                            &sources,
                            &node.location,
                            active_slot_plan,
                        );
                    }

                    Ok(OwnedRuntimeTemplateNode::Slot {
                        location: node.location,
                    })
                }

                TemplateIrNodeKind::InsertContribution { template } => {
                    Ok(OwnedRuntimeTemplateNode::ChildTemplate {
                        template: Box::new(
                            materializer.materialize_template(template, active_slot_plan, None)?,
                        ),
                    })
                }
            }
        })?;

        increment_ast_counter(AstCounter::RuntimeSlotHandoffOwnedNodesMaterialized);
        Ok(owned_node)
    }

    fn with_template_on_stack<T>(
        &mut self,
        template_ref: TemplateRef,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        if self.template_stack.contains(&template_ref) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff materialization found a recursive child template.",
            ));
        }

        self.template_stack.push(template_ref);
        let result = build(self);
        self.template_stack.pop();
        result
    }

    fn with_node_on_stack<T>(
        &mut self,
        id: TemplateIrNodeId,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        if self.node_stack.contains(&id) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff materialization found a recursive node reference.",
            ));
        }

        self.node_stack.push(id);
        let result = build(self);
        self.node_stack.pop();
        result
    }

    fn get_template(&self, id: TemplateIrId) -> Result<&TemplateIr, CompilerError> {
        self.store.get_template(id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff materialization referenced a missing template.",
            )
        })
    }

    fn get_node(&self, id: TemplateIrNodeId) -> Result<&TemplateIrNode, CompilerError> {
        self.store.get_node(id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff materialization referenced a missing node.",
            )
        })
    }

    fn effective_node(&self, id: TemplateIrNodeId) -> Result<TemplateIrNode, CompilerError> {
        self.get_node(id).cloned()
    }

    /// Resolves the effective expression for a site from the active root-first
    /// template overlay stack.
    ///
    /// WHAT: searches active overlay sets from the finalized outer root toward
    ///       nested child-template references and returns the first expression
    ///       override for `site_id`. Falls back to the structural expression
    ///       when no active overlay owns the site.
    /// WHY: finalization writes one root expression overlay for every reachable
    ///      site. Child references still own their slot and wrapper dimensions,
    ///      but must not hide a root-level annotation or normalization override.
    fn effective_expression(
        &self,
        site_id: ExpressionSiteId,
        fallback: &Expression,
    ) -> Result<Expression, CompilerError> {
        Ok(self
            .effective_expression_for_site(site_id)?
            .unwrap_or_else(|| fallback.clone()))
    }

    fn effective_expression_for_site(
        &self,
        site_id: ExpressionSiteId,
    ) -> Result<Option<Expression>, CompilerError> {
        if self.overlay_set_stack.is_empty() {
            return Ok(None);
        }
        let Some(registry_rc) = self.registry.as_ref() else {
            return Err(CompilerError::compiler_error(
                "HIR handoff materialization has an active overlay stack but no registry for expression resolution.",
            ));
        };

        let registry = registry_rc.borrow();
        Ok(registry
            .expression_for_overlay_stack(&self.overlay_set_stack, site_id)?
            .cloned())
    }

    /// Resolves the effective wrapper context for a child-template occurrence,
    /// preferring the override at the top of the body-root overlay stack.
    ///
    /// WHAT: looks up the current body-root overlay set in the registry and
    ///       returns a clone of the wrapper context for `occurrence_id` if one
    ///       exists. Returns `None` when there is no overlay set or no
    ///       wrapper-context overlay. An active overlay without a registry is
    ///       an internal error.
    /// WHY: this mirrors `effective_expression_for_site` for the wrapper-context
    ///      dimension so child-template handoff can apply inherited `$children(..)`
    ///      wrappers and `$fresh` suppression without mutating the structural root.
    fn effective_wrapper_context_for_occurrence(
        &self,
        occurrence_id: ChildTemplateOccurrenceId,
    ) -> Result<Option<TirWrapperContext>, CompilerError> {
        let Some(overlay_set_id) = self.overlay_set_stack.last().copied() else {
            return Ok(None);
        };
        let Some(registry_rc) = self.registry.as_ref() else {
            return Err(CompilerError::compiler_error(
                "HIR handoff materialization has an active overlay stack but no registry for wrapper-context resolution.",
            ));
        };

        let registry = registry_rc.borrow();
        let overlay_set = registry.overlay_set(overlay_set_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HIR handoff materialization referenced missing overlay set {}",
                overlay_set_id
            ))
        })?;
        let Some(wrapper_context_overlay_id) = overlay_set.wrapper_context else {
            return Ok(None);
        };
        let wrapper_context_overlay = registry
            .wrapper_context_overlay(wrapper_context_overlay_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HIR handoff materialization referenced missing wrapper-context overlay {}",
                    wrapper_context_overlay_id
                ))
            })?;
        Ok(wrapper_context_overlay
            .context_for_occurrence(occurrence_id)
            .cloned())
    }

    /// Resolves the effective slot resolution for a slot occurrence,
    /// preferring the resolution at the top of the body-root overlay stack.
    ///
    /// WHAT: looks up the current body-root overlay set in the registry and
    ///       returns a clone of the `TirSlotResolution` for `occurrence_id` if one
    ///       exists. Returns `None` when there is no overlay set or no
    ///       slot-resolution overlay. An active overlay without a registry is
    ///       an internal error.
    /// WHY: this mirrors `effective_expression_for_site` and
    ///      `effective_wrapper_context_for_occurrence` for the slot-resolution
    ///      dimension so handoff materialization can render resolved slot fills
    ///      from the final effective view instead of treating every structural
    ///      `Slot` node as a no-output placeholder.
    fn effective_slot_resolution_for_occurrence(
        &self,
        occurrence_id: SlotOccurrenceId,
    ) -> Result<Option<super::overlays::TirSlotResolution>, CompilerError> {
        let Some(overlay_set_id) = self.overlay_set_stack.last().copied() else {
            return Ok(None);
        };
        let Some(registry_rc) = self.registry.as_ref() else {
            return Err(CompilerError::compiler_error(
                "HIR handoff materialization has an active overlay stack but no registry for slot-resolution lookup.",
            ));
        };

        let registry = registry_rc.borrow();
        let overlay_set = registry.overlay_set(overlay_set_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HIR handoff materialization referenced missing overlay set {}",
                overlay_set_id
            ))
        })?;
        let Some(slot_resolution_overlay_id) = overlay_set.slot_resolution else {
            return Ok(None);
        };
        let slot_resolution_overlay = registry
            .slot_resolution_overlay(slot_resolution_overlay_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HIR handoff materialization referenced missing slot-resolution overlay {}",
                    slot_resolution_overlay_id
                ))
            })?;
        Ok(slot_resolution_overlay
            .resolution_for_occurrence(occurrence_id)
            .cloned())
    }

    fn effective_branch_selector(
        &self,
        selector: &TemplateBranchSelector,
        site_id: ExpressionSiteId,
    ) -> Result<TemplateBranchSelector, CompilerError> {
        let Some(expression) = self.effective_expression_for_site(site_id)? else {
            return Ok(selector.clone());
        };

        Ok(match selector {
            TemplateBranchSelector::Bool(_) => TemplateBranchSelector::Bool(expression),
            TemplateBranchSelector::OptionPresentCapture { pattern, .. } => {
                TemplateBranchSelector::OptionPresentCapture {
                    scrutinee: expression,
                    pattern: pattern.clone(),
                }
            }
        })
    }

    fn effective_loop_header(
        &self,
        header: &TemplateLoopHeader,
        header_sites: TemplateLoopHeaderExpressionSites,
    ) -> Result<TemplateLoopHeader, CompilerError> {
        Ok(match (header, header_sites) {
            (
                TemplateLoopHeader::Conditional { condition },
                TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
            ) => TemplateLoopHeader::Conditional {
                condition: Box::new(
                    self.effective_expression_for_site(site_id)?
                        .unwrap_or_else(|| condition.as_ref().clone()),
                ),
            },

            (
                TemplateLoopHeader::Range { bindings, range },
                TemplateLoopHeaderExpressionSites::Range { start, end, step },
            ) => {
                let mut range = range.as_ref().clone();
                if let Some(expression) = self.effective_expression_for_site(start)? {
                    range.start = expression;
                }
                if let Some(expression) = self.effective_expression_for_site(end)? {
                    range.end = expression;
                }
                if let Some(step_site_id) = step
                    && let Some(expression) = self.effective_expression_for_site(step_site_id)?
                {
                    range.step = Some(expression);
                }

                TemplateLoopHeader::Range {
                    bindings: bindings.clone(),
                    range: Box::new(range),
                }
            }

            (
                TemplateLoopHeader::Collection { bindings, iterable },
                TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
            ) => TemplateLoopHeader::Collection {
                bindings: bindings.clone(),
                iterable: Box::new(
                    self.effective_expression_for_site(site_id)?
                        .unwrap_or_else(|| iterable.as_ref().clone()),
                ),
            },

            _ => header.clone(),
        })
    }

    /// Materializes a `ChildTemplate` node into an owned runtime handoff node,
    /// preferring the stable folded-text shortcut when it is available.
    ///
    /// WHAT: tries `materialize_folded_child_text` first so const-foldable
    ///       same-store children become owned `Text` nodes. That shortcut is
    ///       tied to `self.store` — it folds through the current store's string
    ///       table and fold context — so foreign children fall through to
    ///       owning-store materialization via the module-local registry and a
    ///       nested materializer scoped to the foreign store. Same-store
    ///       children use the current materializer directly.
    /// WHY: both the wrapper-context and non-wrapper-context paths need the same
    ///      child handoff shape, so factoring it avoids duplicating the fold
    ///      shortcut and the cross-store resolution logic.
    fn materialize_child_template_node(
        &mut self,
        reference: &TemplateTirChildReference,
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        // Same-store folded-text shortcut: inline stable const-foldable
        // children as owned `Text` nodes before any structural materialization.
        // Folding uses the current store's string table and fold context, so
        // it only applies when the child lives in the same store.
        if injection.is_none()
            && let Some(text_node) = self.materialize_folded_child_text(reference, location)?
        {
            return Ok(text_node);
        }

        let is_cross_store = reference.root.store_id != self.store.store_id();

        if is_cross_store {
            return self.materialize_cross_store_child(reference);
        }

        // Same-store child: resolve the store-local ID and materialize through
        // the current materializer, preserving the parent's active slot plan
        // for runtime-slot-site validation.
        let template_id = reference
            .template_id_in_store(self.store.store_id())
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "HIR handoff: child template reference is not in the current store.",
                )
            })?;

        // Push the child's overlay set so effective expression, slot
        // resolution, and wrapper context lookups during materialization
        // read through the child's overlay context rather than the parent's.
        // Without this, child templates with expression or slot overlays
        // would materialize from stale structural payloads. The scoped helper
        // validates registry authority and restores the stack on errors.
        let handoff = if reference.phase.is_at_least(TemplateTirPhase::Composed) {
            self.with_overlay_set(reference.overlay_set_id, |materializer| {
                materializer.materialize_template(template_id, active_slot_plan, injection)
            })
        } else {
            self.materialize_template(template_id, active_slot_plan, injection)
        };

        Ok(OwnedRuntimeTemplateNode::ChildTemplate {
            template: Box::new(handoff?),
        })
    }

    /// Materializes a foreign (cross-store) child reference through its owning
    /// store using a nested materializer.
    ///
    /// WHAT: resolves the foreign store and template from the module-local
    ///       registry, then creates a nested `RuntimeHandoffMaterializer`
    ///       scoped to that store. The nested materializer inherits the
    ///       parent's qualified template stack so A-store -> B-store -> A-store
    ///       cycles are detected. The parent's `active_slot_plan` is not
    ///       forwarded because `TemplateSlotPlanId` is store-local.
    /// WHY: foreign child references must not be interpreted as IDs in the
    ///      composition store. Returning precise errors for missing
    ///      registry, store, or template prevents silent mis-materialization.
    fn materialize_cross_store_child(
        &mut self,
        reference: &TemplateTirChildReference,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let registry = self
            .registry
            .as_ref()
            .map(Rc::clone)
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR HIR handoff: cross-store child template requires a registry, but none is available.",
                )
            })?;

        let foreign_store_rc = {
            let registry_borrow = registry.borrow();
            registry_borrow
                .store_handle(reference.root.store_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "TIR HIR handoff: cross-store child template store {} not found in registry.",
                        reference.root.store_id
                    ))
                })?
        };

        let foreign_store_borrow = foreign_store_rc.borrow();

        // Verify the referenced template exists in its owning store before
        // materializing, so a missing template produces a precise error
        // instead of a generic "missing template" deep in recursive traversal.
        if foreign_store_borrow
            .get_template(reference.root.template_id)
            .is_none()
        {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff: cross-store child template {} not found in store {}.",
                reference.root.template_id, reference.root.store_id
            )));
        }

        // The nested materializer borrows the foreign store and inherits the
        // ancestor template stack for qualified cycle detection. The child's
        // overlay set is pushed so expression, slot-resolution, and
        // wrapper-context lookups read through the child's overlay context.
        let mut nested = self.nested_foreign_store_materializer(
            &foreign_store_borrow,
            reference
                .phase
                .is_at_least(TemplateTirPhase::Composed)
                .then_some(reference.overlay_set_id),
        )?;

        // `TemplateSlotPlanId` is store-local: do not forward the parent's
        // active slot plan. The foreign template's own slot plan (if any) is
        // resolved inside `materialize_template`.
        let handoff = nested.materialize_template(reference.root.template_id, None, None)?;

        Ok(OwnedRuntimeTemplateNode::ChildTemplate {
            template: Box::new(handoff),
        })
    }

    /// Materializes a list of resolved slot sources into owned runtime handoff
    /// nodes.
    ///
    /// WHAT: a single source becomes one owned node; multiple sources become a
    ///       `Sequence` of child-template handoffs in deterministic source order.
    /// WHY: repeated slots and multi-source contributions are represented by a
    ///      list of sources in the overlay; the handoff must preserve that order
    ///      without inventing new node kinds.
    fn materialize_resolved_slot_sources(
        &mut self,
        sources: &[TemplateRef],
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        if sources.is_empty() {
            return Ok(OwnedRuntimeTemplateNode::Slot {
                location: location.to_owned(),
            });
        }

        if sources.len() == 1 {
            return self.materialize_resolved_slot_source(&sources[0], location, active_slot_plan);
        }

        let mut children = Vec::with_capacity(sources.len());
        for source in sources {
            children.push(self.materialize_resolved_slot_source(
                source,
                location,
                active_slot_plan,
            )?);
        }

        Ok(OwnedRuntimeTemplateNode::Sequence { children })
    }

    /// Materializes one resolved slot source into an owned runtime handoff node.
    ///
    /// WHAT: constructs a same-store `TemplateTirChildReference` from the resolved
    ///       `TemplateRef` and delegates to `materialize_child_template_node` so
    ///       const-foldable sources inline as owned `Text` nodes and runtime sources
    ///       become nested `ChildTemplate` handoffs.
    /// WHY: slot-resolution overlays carry bare `TemplateRef` sources without phase
    ///      or overlay context; routing them through the same child-materialization
    ///      path as `ChildTemplate` nodes keeps fold shortcuts and cross-store
    ///      validation consistent.
    fn materialize_resolved_slot_source(
        &mut self,
        source: &TemplateRef,
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let child_reference = TemplateTirChildReference::new(
            *source,
            TemplateTirPhase::Composed,
            TemplateOverlaySetId::empty(),
        );
        self.materialize_child_template_node(&child_reference, location, active_slot_plan, None)
    }

    /// Applies a wrapper-context overlay entry to an already-materialized child
    /// handoff node.
    ///
    /// WHAT: validates the wrapper-context shape, honors `$fresh` suppression, and
    ///       resolves the inherited wrapper set into same-store wrapper refs before
    ///       wrapping the child handoff. `IfChildEmits` becomes a neutral
    ///       `ConditionalWrapper` node so HIR can use its existing emitted-output
    ///       guard without seeing TIR overlay state.
    /// WHY: this is the runtime-handoff analogue of
    ///      `apply_wrapper_context_overlay_to_child_emission` in `fold.rs`.
    fn apply_wrapper_context_overlay_to_child_handoff(
        &mut self,
        context: &TirWrapperContext,
        child_handoff: OwnedRuntimeTemplateNode,
        child_location: &SourceLocation,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        if context.skip_parent_child_wrappers {
            return Ok(child_handoff);
        }

        let Some(wrapper_set_ref) = context.inherited_wrapper_set else {
            return Ok(child_handoff);
        };

        if wrapper_set_ref.store_id != self.store.store_id() {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff: inherited wrapper set is not in the current store.",
            ));
        }

        let wrapper_set = self
            .store
            .get_wrapper_set(wrapper_set_ref.wrapper_set_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR HIR handoff: inherited wrapper set referenced by overlay is missing.",
                )
            })?;

        let wrapper_references: Vec<TemplateWrapperReference> = wrapper_set.wrappers.clone();

        match context.application_mode {
            TirWrapperApplicationMode::Always => self
                .apply_wrapper_templates_around_child_handoff(&wrapper_references, child_handoff),

            TirWrapperApplicationMode::IfChildEmits => self
                .apply_conditional_wrapper_templates_around_child_handoff(
                    &wrapper_references,
                    child_handoff,
                    child_location,
                ),
        }
    }

    /// Wraps a child handoff node in each inherited wrapper template.
    ///
    /// WHAT: iterates wrappers in reverse (outermost-first), composing each
    ///       wrapper around the current wrapped child. The result is an owned
    ///       runtime node that represents wrapper-text-around-child.
    /// WHY: this mirrors `fold_conditional_child_wrappers_around_emission` and
    ///      the structural `wrap_tir_node_in_wrappers` nesting order.
    fn apply_wrapper_templates_around_child_handoff(
        &mut self,
        wrapper_references: &[TemplateWrapperReference],
        child_handoff: OwnedRuntimeTemplateNode,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let mut current = child_handoff;
        for wrapper_reference in wrapper_references.iter().rev() {
            current = self
                .apply_single_wrapper_template_around_child_handoff(*wrapper_reference, current)?;
        }
        Ok(current)
    }

    /// Builds one output-conditioned wrapper node for an inherited wrapper set.
    ///
    /// WHAT: materializes all wrappers around an `AggregateOutput` marker, then
    /// pairs that wrapper tree with the original child in `ConditionalWrapper`.
    /// WHY: `IfChildEmits` is a runtime structural condition. HIR already knows
    /// how to append aggregate wrappers only when a source accumulator emitted
    /// output, so the handoff should expose that neutral shape instead of TIR
    /// overlay state.
    fn apply_conditional_wrapper_templates_around_child_handoff(
        &mut self,
        wrapper_references: &[TemplateWrapperReference],
        child_handoff: OwnedRuntimeTemplateNode,
        child_location: &SourceLocation,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        if wrapper_references.is_empty() {
            return Ok(child_handoff);
        }

        let mut wrapper = OwnedRuntimeTemplateNode::AggregateOutput;
        for wrapper_reference in wrapper_references.iter().rev() {
            wrapper = self
                .apply_single_wrapper_template_around_child_handoff(*wrapper_reference, wrapper)?;
        }

        Ok(OwnedRuntimeTemplateNode::ConditionalWrapper {
            child: Box::new(child_handoff),
            wrapper: Box::new(wrapper),
            location: child_location.to_owned(),
        })
    }

    /// Materializes a wrapper around a child handoff using the given materializer.
    ///
    /// WHAT: consolidates the duplicated cross-store/same-store match into one
    ///       path that uses a materializer reference regardless of which store
    ///       owns the wrapper template.
    /// WHY: eliminates the duplicated `match fill_target_key` block while
    ///      preserving the existing semantics for both same-store and cross-store
    ///      wrapper materialization.
    fn materialize_wrapper_with_child(
        materializer: &mut RuntimeHandoffMaterializer,
        wrapper_root: TemplateIrNodeId,
        fill_target_key: Option<SlotKey>,
        child_handoff: OwnedRuntimeTemplateNode,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        match fill_target_key {
            Some(fill_target_key) => materializer.materialize_node_with_injection(
                wrapper_root,
                None,
                Some((&fill_target_key, &child_handoff)),
            ),
            None => {
                let wrapper_content = materializer.materialize_node(wrapper_root, None)?;
                Ok(OwnedRuntimeTemplateNode::Sequence {
                    children: vec![wrapper_content, child_handoff],
                })
            }
        }
    }

    /// Wraps one wrapper template around a child handoff node.
    ///
    /// WHAT: materializes the wrapper template's content, then either injects the
    ///       child at the wrapper's loose-fill slot or appends it after wrapper
    ///       content when the schema has no loose-fill target (slot-less or
    ///       named-only wrappers).
    ///       Runtime slot-plan wrappers are rejected because inherited `$children(..)`
    ///       wrappers must be ordinary render templates.
    /// WHY: this produces the same owned shape as TIR wrapper composition
    ///      without exposing TIR identity across the HIR boundary.
    fn apply_single_wrapper_template_around_child_handoff(
        &mut self,
        wrapper_reference: TemplateWrapperReference,
        child_handoff: OwnedRuntimeTemplateNode,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        // Resolve the wrapper template and its owning store, supporting cross-store
        // references through the module-local registry.
        let is_cross_store = wrapper_reference.root.store_id != self.store.store_id();

        let wrapper_store_rc = if is_cross_store {
            let registry = self
                .registry
                .as_ref()
                .map(Rc::clone)
                .ok_or_else(|| {
                    CompilerError::compiler_error(
                        "TIR HIR handoff: cross-store wrapper requires a registry, but none is available.",
                    )
                })?;
            let registry_borrow = registry.borrow();
            Some(
                registry_borrow
                    .store_handle(wrapper_reference.root.store_id)
                    .ok_or_else(|| {
                        CompilerError::compiler_error(
                            "TIR HIR handoff: cross-store wrapper store not found in registry.",
                        )
                    })?,
            )
        } else {
            None
        };

        let wrapper_store_borrow;
        let wrapper_store: &TemplateIrStore = if let Some(ref rc) = wrapper_store_rc {
            wrapper_store_borrow = rc.borrow();
            &wrapper_store_borrow
        } else {
            self.store
        };

        let (wrapper_root, has_runtime_slot_plan) = wrapper_store
            .get_template(wrapper_reference.root.template_id)
            .map(|template| (template.root, template.runtime_slot_plan.is_some()))
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR HIR handoff: wrapper template {} not found in store {}.",
                    wrapper_reference.root.template_id, wrapper_reference.root.store_id
                ))
            })?;

        if has_runtime_slot_plan {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff: inherited wrapper template declares a runtime slot plan.",
            ));
        }

        let schema = collect_tir_slot_schema(wrapper_store, wrapper_reference.root.template_id)
            .map_err(CompilerError::from)?;
        let fill_target_key = schema.loose_fill_target_key();

        if is_cross_store {
            // The nested materializer inherits the parent's qualified template
            // stack so cross-store wrapper cycles are detected the same way as
            // cross-store child cycles. The registry was already verified when
            // resolving `wrapper_store_rc` above.
            let mut wrapper_materializer = self.nested_foreign_store_materializer(
                wrapper_store,
                wrapper_reference
                    .phase
                    .is_at_least(TemplateTirPhase::Composed)
                    .then_some(wrapper_reference.overlay_set_id),
            )?;

            Self::materialize_wrapper_with_child(
                &mut wrapper_materializer,
                wrapper_root,
                fill_target_key,
                child_handoff,
            )
        } else {
            if wrapper_reference
                .phase
                .is_at_least(TemplateTirPhase::Composed)
            {
                self.with_overlay_set(wrapper_reference.overlay_set_id, |materializer| {
                    Self::materialize_wrapper_with_child(
                        materializer,
                        wrapper_root,
                        fill_target_key,
                        child_handoff,
                    )
                })
            } else {
                Self::materialize_wrapper_with_child(
                    self,
                    wrapper_root,
                    fill_target_key,
                    child_handoff,
                )
            }
        }
    }

    fn get_slot_plan(&self, id: TemplateSlotPlanId) -> Result<&TemplateSlotPlan, CompilerError> {
        self.store.get_slot_plan(id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff materialization referenced a missing slot plan.",
            )
        })
    }

    fn materialize_folded_child_text(
        &mut self,
        reference: &TemplateTirChildReference,
        location: &SourceLocation,
    ) -> Result<Option<OwnedRuntimeTemplateNode>, CompilerError> {
        // Child below Composed: the fold shortcut requires a composed child
        // root, so younger children fall through to structural handoff.
        if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
            return Ok(None);
        }

        // Foreign-store child: the shortcut folds through the current store's
        // string table, so foreign children fall through to owning-store
        // materialization.
        if reference
            .template_id_in_store(self.store.store_id())
            .is_none()
        {
            return Ok(None);
        }

        // No fold context: the direct-by-ID and slot-handoff paths have no
        // fold context, so the text shortcut is unavailable.
        let Some(fold_context) = self.fold_context.as_deref_mut() else {
            return Ok(None);
        };

        // Active bindings: a child under loop or branch bindings cannot be
        // const-folded.
        if !fold_context.bindings.is_empty() {
            return Ok(None);
        }

        // The view-backed fold-context materializer requires a registry for
        // child-view construction. A missing registry is an internal
        // invariant violation, not a shortcut-unavailable state.
        let Some(registry) = fold_context.template_ir_registry.as_ref().map(Rc::clone) else {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff folded-child text shortcut requires a registry, but the fold context has none.",
            ));
        };

        // The shortcut builds a view for the child's own overlay set, while
        // structural handoff resolves the full root-first overlay stack. An
        // outer expression or slot overlay must stay on the structural path
        // so a valid root-level override is not lost. Wrapper-context-only
        // overlays remain safe because the caller applies them after the
        // folded child emission is produced.
        let registry_borrow = registry.borrow();
        for active_overlay_set_id in self.overlay_set_stack.iter().copied() {
            if active_overlay_set_id == reference.overlay_set_id {
                continue;
            }

            let active_overlay_set = registry_borrow
                .overlay_set(active_overlay_set_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "TIR HIR handoff materialization referenced missing overlay set {}",
                        active_overlay_set_id
                    ))
                })?;
            if active_overlay_set.expression_overrides.is_some()
                || active_overlay_set.slot_resolution.is_some()
            {
                return Ok(None);
            }
        }

        // Propagate child root, phase and overlay-set authority failures.
        // A malformed child overlay must not silently fall through to
        // structural materialization.
        let child_view = TirView::with_minimum_phase(
            &registry_borrow,
            reference.root,
            reference.phase,
            TemplateTirPhase::Composed,
            reference.overlay_set_id,
        )?;

        // Unsafe fold shape: non-linear or overlay-bearing shapes that the
        // const-fold shortcut cannot handle fall through to structural handoff.
        let fold_safe = tir_view_is_empty_overlay_linear_fold_safe(&child_view, self.store)?;
        if !fold_safe {
            return Ok(None);
        }

        // Speculative folding currently reports both source rejection and
        // ordinary runtime-expression ineligibility through `TemplateError`.
        // Neither invalidates the structural handoff path. Required view
        // authority failures have already propagated above.
        match fold_tir_view(&child_view, self.store, fold_context) {
            Ok(TemplateEmission::Output(text)) => {
                let byte_len = fold_context.string_table.resolve(text).len() as u32;
                Ok(Some(OwnedRuntimeTemplateNode::Text {
                    text,
                    byte_len,
                    reactive_subscription: None,
                    location: location.to_owned(),
                }))
            }
            Ok(_) => Ok(None),
            Err(TemplateError::Infrastructure(error)) => Err(*error),

            // Earlier source validation owns ordinary non-const and source
            // diagnostics. Keep those as shortcut-unavailable structural
            // handoff instead of changing their diagnostic lane here.
            Err(TemplateError::Diagnostic(_)) => Ok(None),
        }
    }
}
