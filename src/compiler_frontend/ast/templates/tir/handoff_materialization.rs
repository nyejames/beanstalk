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
    TemplateViewContext, TirSlotResolutionKind, TirWrapperApplicationMode, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::slot_composition::collect_tir_slot_schema;
use crate::compiler_frontend::ast::templates::tir::slot_plan::{
    TemplateSlotPlan, TemplateSlotSiteRenderPiece,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{
    TemplateTirPhase, TirView, TirViewIdentity,
};
use crate::compiler_frontend::ast::templates::tir::{
    fold_tir_view, tir_view_is_empty_overlay_linear_fold_safe,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl TemplateIrStore {
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
    ) -> Result<Option<OwnedRuntimeSlotApplicationHandoff>, CompilerError> {
        let template_id = self.template_id_for_view(view)?;
        let mut materializer = RuntimeHandoffMaterializer::new_with_view(self, view, None);
        materializer.owned_runtime_slot_handoff_for_template(template_id)
    }

    /// Materializes an ordinary runtime template from the caller's finalized
    /// effective view while retaining the existing child-fold shortcut.
    pub(crate) fn owned_runtime_template_handoff_for_tir_view_with_fold_context(
        &self,
        view: &TirView<'_>,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
        let template_id = self.template_id_for_view(view)?;
        let mut materializer =
            RuntimeHandoffMaterializer::new_with_view(self, view, Some(fold_context));
        materializer.owned_runtime_template_handoff_for_template(template_id)
    }

    fn template_id_for_view(&self, view: &TirView<'_>) -> Result<TemplateIrId, CompilerError> {
        let template_id = view.root_ref();
        self.get_template(template_id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff view materialization referenced a missing template.",
            )
        })?;
        Ok(template_id)
    }
}

struct RuntimeHandoffMaterializer<'store, 'context, 'fold> {
    store: &'store TemplateIrStore,
    fold_context: Option<&'context mut TemplateFoldContext<'fold>>,
    /// Exact view for the structural root currently being materialized.
    effective_view: Option<TirView<'store>>,
    template_stack: Vec<TirViewIdentity>,
    node_stack: Vec<(TemplateIrNodeId, TirViewIdentity)>,
}

impl<'store, 'context, 'fold> RuntimeHandoffMaterializer<'store, 'context, 'fold> {
    fn new_with_view(
        store: &'store TemplateIrStore,
        view: &TirView<'store>,
        fold_context: Option<&'context mut TemplateFoldContext<'fold>>,
    ) -> Self {
        Self {
            store,
            fold_context,
            effective_view: Some(view.clone()),
            template_stack: Vec::new(),
            node_stack: Vec::new(),
        }
    }

    /// Validates every named overlay payload before a context becomes active.
    fn validate_context(&self, context: TemplateViewContext) -> Result<(), CompilerError> {
        if let Some(id) = context.expression_overlay
            && self.store.expression_overlay(id).is_none()
        {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff materialization referenced missing expression overlay {id}"
            )));
        }

        if let Some(id) = context.slot_resolution
            && self.store.slot_resolution_overlay(id).is_none()
        {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff materialization referenced missing slot-resolution overlay {id}"
            )));
        }

        if let Some(id) = context.wrapper_context
            && self.store.wrapper_context_overlay(id).is_none()
        {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff materialization referenced missing wrapper-context overlay {id}"
            )));
        }

        Ok(())
    }

    /// Temporarily activates one exact view while materializing a nested root.
    fn with_view<T>(
        &mut self,
        view: TirView<'store>,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        self.validate_context(view.context())?;
        let parent_view = self.effective_view.replace(view);
        let result = build(self);
        self.effective_view = parent_view;
        result
    }

    fn current_view(&self) -> Result<&TirView<'store>, CompilerError> {
        self.effective_view.as_ref().ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff materialization requires an exact TirView.",
            )
        })
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

        self.with_template_on_stack(id, |materializer| {
            materializer.materialize_runtime_slot_application_by_parts(root, slot_plan_id, None)
        })
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

        self.with_template_on_stack(id, |materializer| {
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
        })
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
                    let helper_view = materializer.current_view()?.structural_helper(template)?;
                    let helper_handoff = materializer.with_view(helper_view, |materializer| {
                        materializer.materialize_template(template, active_slot_plan, None)
                    })?;
                    Ok(OwnedRuntimeTemplateNode::ChildTemplate {
                        template: Box::new(helper_handoff),
                    })
                }
            }
        })?;

        increment_ast_counter(AstCounter::RuntimeSlotHandoffOwnedNodesMaterialized);
        Ok(owned_node)
    }

    fn with_template_on_stack<T>(
        &mut self,
        template_ref: TemplateIrId,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        let traversal_key = self.current_view()?.identity();
        if traversal_key.root != template_ref {
            return Err(CompilerError::compiler_error(format!(
                "TIR HIR handoff materialization view root {} does not match template {}.",
                traversal_key.root, template_ref
            )));
        }

        if self.template_stack.contains(&traversal_key) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff materialization found a recursive child template.",
            ));
        }

        self.template_stack.push(traversal_key);
        let result = build(self);
        self.template_stack.pop();
        result
    }

    fn with_node_on_stack<T>(
        &mut self,
        id: TemplateIrNodeId,
        build: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
    ) -> Result<T, CompilerError> {
        let traversal_key = (id, self.current_view()?.identity());
        if self.node_stack.contains(&traversal_key) {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff materialization found a recursive node reference.",
            ));
        }

        self.node_stack.push(traversal_key);
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

    /// Resolves the effective expression for a site from the current exact view.
    ///
    /// WHAT: reads the complete root overlay through `TirView` and falls back
    ///       to the structural expression when the site has no override.
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
        Ok(self
            .current_view()?
            .effective_expression_for_site(site_id)?
            .cloned())
    }

    /// Resolves the effective wrapper context for a child-template occurrence,
    /// preferring the override carried by the current exact view.
    ///
    /// WHAT: reads the active value-carried view context and resolves its
    ///       wrapper-context overlay ID through the module store, returning a
    ///       clone of the wrapper context for `occurrence_id` if one exists.
    ///       Returns `None` when there is no view context or no wrapper-context
    ///       overlay. A missing active overlay is an internal error.
    /// WHY: this mirrors `effective_expression_for_site` for the wrapper-context
    ///      dimension so child-template handoff can apply inherited `$children(..)`
    ///      wrappers and `$fresh` suppression without mutating the structural root.
    fn effective_wrapper_context_for_occurrence(
        &self,
        occurrence_id: ChildTemplateOccurrenceId,
    ) -> Result<Option<TirWrapperContext>, CompilerError> {
        Ok(self
            .current_view()?
            .effective_wrapper_context(occurrence_id)?
            .cloned())
    }

    /// Resolves the effective slot resolution for a slot occurrence,
    /// preferring the resolution carried by the current exact view.
    ///
    /// WHAT: reads the active value-carried view context and resolves its
    ///       slot-resolution overlay ID through the module store, returning a
    ///       clone of the `TirSlotResolution` for `occurrence_id` if one exists.
    ///       Returns `None` when there is no view context or no slot-resolution
    ///       overlay. A missing active overlay is an internal error.
    /// WHY: this mirrors `effective_expression_for_site` and
    ///      `effective_wrapper_context_for_occurrence` for the slot-resolution
    ///      dimension so handoff materialization can render resolved slot fills
    ///      from the final effective view instead of treating every structural
    ///      `Slot` node as a no-output placeholder.
    fn effective_slot_resolution_for_occurrence(
        &self,
        occurrence_id: SlotOccurrenceId,
    ) -> Result<Option<super::overlays::TirSlotResolution>, CompilerError> {
        Ok(self
            .current_view()?
            .effective_slot_resolution(occurrence_id)?
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
    /// WHAT: enters the exact structural child view once, then tries
    ///       `materialize_folded_view_text` so const-foldable
    ///       children become owned `Text` nodes, then materializes the
    ///       store-local child structurally when folding is unavailable.
    /// WHY: both the wrapper-context and non-wrapper-context paths need the same
    ///      child handoff shape, so factoring it avoids duplicating the fold
    ///      shortcut and structural materialization.
    fn materialize_child_template_node(
        &mut self,
        reference: &TemplateTirChildReference,
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let child_view = self.current_view()?.structural_child(*reference)?;
        self.materialize_child_template_node_with_view(
            reference.root,
            child_view,
            location,
            active_slot_plan,
            injection,
        )
    }

    fn materialize_child_template_node_with_view(
        &mut self,
        template_id: TemplateIrId,
        child_view: TirView<'store>,
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
        injection: Option<(&SlotKey, &OwnedRuntimeTemplateNode)>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        // Folded-text shortcut: inline stable const-foldable children as owned
        // `Text` nodes before any structural materialization.
        if injection.is_none()
            && let Some(text_node) = self.materialize_folded_view_text(&child_view, location)?
        {
            return Ok(text_node);
        }

        let handoff = self.with_view(child_view, |materializer| {
            materializer.materialize_template(template_id, active_slot_plan, injection)
        });

        Ok(OwnedRuntimeTemplateNode::ChildTemplate {
            template: Box::new(handoff?),
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
        sources: &[TemplateIrId],
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
    /// WHAT: enters the resolved source exactly once, then materializes that exact
    ///       view so const-foldable sources can inline as owned `Text` nodes and
    ///       runtime sources retain the owned `ChildTemplate` handoff shape.
    /// WHY: slot-resolution overlays carry bare `TemplateIrId` sources. Their
    ///      phase and context are supplied by the active parent view, so a
    ///      synthetic child reference would apply the structural transition twice.
    fn materialize_resolved_slot_source(
        &mut self,
        source: &TemplateIrId,
        location: &SourceLocation,
        active_slot_plan: Option<TemplateSlotPlanId>,
    ) -> Result<OwnedRuntimeTemplateNode, CompilerError> {
        let source_view = self.current_view()?.resolved_slot_source(*source)?;
        self.materialize_child_template_node_with_view(
            *source,
            source_view,
            location,
            active_slot_plan,
            None,
        )
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

        let wrapper_set = self.store.get_wrapper_set(wrapper_set_ref).ok_or_else(|| {
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
    /// WHAT: consolidates wrapper materialization into one path that uses a
    ///       materializer reference for the module-local wrapper template.
    /// WHY: eliminates the duplicated `match fill_target_key` block while
    ///      preserving wrapper materialization semantics.
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
        let wrapper_store = self.store;

        let (wrapper_root, has_runtime_slot_plan) = wrapper_store
            .get_template(wrapper_reference.root)
            .map(|template| (template.root, template.runtime_slot_plan.is_some()))
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR HIR handoff: wrapper template {} not found in the store.",
                    wrapper_reference.root
                ))
            })?;

        if has_runtime_slot_plan {
            return Err(CompilerError::compiler_error(
                "TIR HIR handoff: inherited wrapper template declares a runtime slot plan.",
            ));
        }

        let schema = collect_tir_slot_schema(wrapper_store, wrapper_reference.root)
            .map_err(CompilerError::from)?;
        let fill_target_key = schema.loose_fill_target_key();

        let wrapper_view = self.current_view()?.wrapper(wrapper_reference)?;
        self.with_view(wrapper_view, |materializer| {
            Self::materialize_wrapper_with_child(
                materializer,
                wrapper_root,
                fill_target_key,
                child_handoff,
            )
        })
    }

    fn get_slot_plan(&self, id: TemplateSlotPlanId) -> Result<&TemplateSlotPlan, CompilerError> {
        self.store.get_slot_plan(id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR HIR handoff materialization referenced a missing slot plan.",
            )
        })
    }

    fn materialize_folded_view_text(
        &mut self,
        child_view: &TirView<'store>,
        location: &SourceLocation,
    ) -> Result<Option<OwnedRuntimeTemplateNode>, CompilerError> {
        // Child below Composed: the fold shortcut requires a composed child
        // root, so younger children fall through to structural handoff.
        if !child_view.phase().is_at_least(TemplateTirPhase::Composed) {
            return Ok(None);
        }

        // The shortcut is safe only when the exact child view has no
        // expression or slot authority that the owned handoff would need.

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
        if child_view.context().expression_overlay.is_some()
            || child_view.context().slot_resolution.is_some()
        {
            return Ok(None);
        }

        // Propagate child root, phase and view context authority failures.
        // A malformed child overlay must not silently fall through to
        // structural materialization.
        // Unsafe fold shape: non-linear or overlay-bearing shapes that the
        // const-fold shortcut cannot handle fall through to structural handoff.
        let fold_safe = tir_view_is_empty_overlay_linear_fold_safe(child_view, self.store)?;
        if !fold_safe {
            return Ok(None);
        }

        // Speculative folding currently reports both source rejection and
        // ordinary runtime-expression ineligibility through `TemplateError`.
        // Neither invalidates the structural handoff path. Required view
        // authority failures have already propagated above.
        match fold_tir_view(child_view, self.store, fold_context) {
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
