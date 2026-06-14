//! Statement-level borrow transfer rules.
//!
//! WHAT: applies borrow effects for HIR statements and records statement/value facts.
//! WHY: keeping statement transfer separate from traversal helpers makes the block transfer
//! entrypoint easier to inspect without changing the borrow-analysis model.

use super::*;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::HirMapOp;
use crate::compiler_frontend::hir::ids::LocalId;

pub(crate) fn transfer_statement(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    block_id: BlockId,
    statement: &HirStatement,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), BorrowCheckError> {
    // WHAT: transfer one statement at the block frontier.
    // WHY: the fixed-point driver merges only block states, so statement effects must be exact.
    let mut tracker = StatementAccessTracker::new(layout.local_count());
    let mut reactive_invalidations = Vec::new();
    let conflicts_before = stats.conflicts_checked;
    let statement_order = layout.statement_order_or_unknown(statement.id);

    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            let location = context.diagnostics.statement_error_location(statement);
            let pending_invalidations = reactive_assignment_invalidations(
                context,
                layout,
                state,
                statement.id,
                target,
                location.clone(),
            )?;

            {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: statement_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_place_indices(
                    &mut read_env,
                    target,
                    location.clone(),
                    &mut RootSet::empty(layout.local_count()),
                )?;
            }
            {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: statement_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_expression(
                    &mut read_env,
                    value,
                    location.clone(),
                    &mut RootSet::empty(layout.local_count()),
                )?;
            }

            transfer_assign_target(
                &mut AssignTransferContext {
                    context,
                    layout,
                    state,
                    block_id,
                    current_order: statement_order,
                    tracker: &mut tracker,
                    location,
                    stats,
                },
                target,
                value,
            )?;
            reactive_invalidations.extend(pending_invalidations);
        }

        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            let location = context.diagnostics.statement_error_location(statement);
            let semantics = resolve_call_semantics(context, target, args.len(), location.clone())?;
            let call_args = args
                .iter()
                .zip(semantics.arg_effects.iter().copied())
                .map(|(argument, effect)| CallArgumentTransfer { argument, effect })
                .collect::<Vec<_>>();
            let pending_invalidations = reactive_mutable_call_invalidations(
                context,
                layout,
                state,
                statement.id,
                target,
                &call_args,
                location.clone(),
            )?;

            transfer_call_arguments_and_result(
                &mut CallTransferContext {
                    context,
                    layout,
                    state,
                    block_id,
                    current_order: statement_order,
                    tracker: &mut tracker,
                    location,
                    stats,
                    value_fact_buffer,
                },
                &call_args,
                *result,
                semantics.return_alias,
            )?;
            reactive_invalidations.extend(pending_invalidations);
        }

        HirStatementKind::MapOp {
            op,
            receiver,
            args,
            result,
        } => {
            let location = context.diagnostics.statement_error_location(statement);
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(CallArgumentTransfer {
                argument: receiver,
                effect: if op.requires_mutable_receiver() {
                    ArgEffect::MutableBorrow
                } else {
                    ArgEffect::SharedBorrow
                },
            });
            for (arg_index, arg) in args.iter().enumerate() {
                call_args.push(CallArgumentTransfer {
                    argument: arg,
                    effect: map_argument_effect(*op, arg_index),
                });
            }
            let pending_invalidations = reactive_map_mutation_invalidations(
                context,
                layout,
                state,
                statement.id,
                *op,
                receiver,
                location.clone(),
            )?;

            transfer_call_arguments_and_result(
                &mut CallTransferContext {
                    context,
                    layout,
                    state,
                    block_id,
                    current_order: statement_order,
                    tracker: &mut tracker,
                    location,
                    stats,
                    value_fact_buffer,
                },
                &call_args,
                *result,
                map_result_alias(*op),
            )?;
            reactive_invalidations.extend(pending_invalidations);
        }

        HirStatementKind::CastOp { source, result, .. } => {
            let location = context.diagnostics.statement_error_location(statement);
            transfer_call_arguments_and_result(
                &mut CallTransferContext {
                    context,
                    layout,
                    state,
                    block_id,
                    current_order: statement_order,
                    tracker: &mut tracker,
                    location,
                    stats,
                    value_fact_buffer,
                },
                &[CallArgumentTransfer {
                    argument: source,
                    effect: ArgEffect::SharedBorrow,
                }],
                *result,
                CallResultAlias::Fresh,
            )?;
        }

        HirStatementKind::Expr(expression) => {
            let location = context.diagnostics.statement_error_location(statement);
            let mut read_env = SharedReadEnv {
                context,
                layout,
                state,
                tracker: &mut tracker,
                location: location.clone(),
                current_order: statement_order,
                stats,
                value_fact_buffer,
            };
            record_shared_reads_in_expression(
                &mut read_env,
                expression,
                location.clone(),
                &mut RootSet::empty(layout.local_count()),
            )?;
        }

        HirStatementKind::Drop(_local) => {
            // Ownership/drop semantics are handled by later analyses.
        }

        HirStatementKind::PushRuntimeFragment { value, .. } => {
            let location = context.diagnostics.statement_error_location(statement);
            let mut read_env = SharedReadEnv {
                context,
                layout,
                state,
                tracker: &mut tracker,
                location: location.clone(),
                current_order: statement_order,
                stats,
                value_fact_buffer,
            };
            record_shared_reads_in_expression(
                &mut read_env,
                value,
                location,
                &mut RootSet::empty(layout.local_count()),
            )?;
        }
    }

    // Aggregate literal children must be moved into the constructed value.
    match &statement.kind {
        HirStatementKind::Assign { value, .. } => {
            transfer_aggregate_expression_ownership(
                layout,
                state,
                value,
                block_id,
                statement_order,
                context.diagnostics.statement_error_location(statement),
                &context.diagnostics,
            )?;
        }
        HirStatementKind::Expr(expression) => {
            transfer_aggregate_expression_ownership(
                layout,
                state,
                expression,
                block_id,
                statement_order,
                context.diagnostics.value_error_location(
                    expression.id,
                    context.diagnostics.statement_error_location(statement),
                ),
                &context.diagnostics,
            )?;
        }
        HirStatementKind::PushRuntimeFragment { value, .. } => {
            transfer_aggregate_expression_ownership(
                layout,
                state,
                value,
                block_id,
                statement_order,
                context.diagnostics.value_error_location(
                    value.id,
                    context.diagnostics.statement_error_location(statement),
                ),
                &context.diagnostics,
            )?;
        }
        HirStatementKind::CastOp { source, .. } => {
            transfer_aggregate_expression_ownership(
                layout,
                state,
                source,
                block_id,
                statement_order,
                context.diagnostics.value_error_location(
                    source.id,
                    context.diagnostics.statement_error_location(statement),
                ),
                &context.diagnostics,
            )?;
        }
        _ => {}
    }

    let statement_fact = StatementBorrowFact {
        shared_roots: roots_to_local_ids(layout, &tracker.shared_roots),
        mutable_roots: roots_to_local_ids(layout, &tracker.mutable_roots),
        conflicts_checked: stats.conflicts_checked - conflicts_before,
    };
    stats.statement_facts.push((statement.id, statement_fact));
    stats
        .reactive_invalidations
        .push((statement.id, reactive_invalidations));

    Ok(())
}

struct CallArgumentTransfer<'a> {
    argument: &'a HirExpression,
    effect: ArgEffect,
}

struct CallTransferContext<'a, 'module, 'state, 'tracker, 'stats, 'facts> {
    context: &'a BorrowTransferContext<'module>,
    layout: &'a FunctionLayout,
    state: &'state mut BorrowState,
    block_id: BlockId,
    current_order: i32,
    tracker: &'tracker mut StatementAccessTracker,
    location: SourceLocation,
    stats: &'stats mut BlockTransferStats,
    value_fact_buffer: &'facts mut ValueFactBuffer,
}

fn transfer_call_arguments_and_result(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    args: &[CallArgumentTransfer<'_>],
    result: Option<LocalId>,
    return_alias: CallResultAlias,
) -> Result<(), BorrowCheckError> {
    if args
        .iter()
        .any(|arg| !matches!(arg.effect, ArgEffect::SharedBorrow))
    {
        input.stats.mutable_call_sites += 1;
    }

    let mut arg_roots = vec![RootSet::empty(input.layout.local_count()); args.len()];

    for (arg_index, arg) in args.iter().enumerate() {
        let argument = arg.argument;
        let argument_location = input
            .context
            .diagnostics
            .value_error_location(argument.id, input.location.clone());

        record_call_argument_reads(
            input,
            argument,
            arg.effect,
            &argument_location,
            &mut arg_roots[arg_index],
        )?;
        transfer_call_argument_access(input, argument, arg.effect, argument_location)?;
    }

    transfer_call_result_alias(input, result, return_alias, &arg_roots)
}

fn record_call_argument_reads(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    argument: &HirExpression,
    effect: ArgEffect,
    argument_location: &SourceLocation,
    arg_roots: &mut RootSet,
) -> Result<(), BorrowCheckError> {
    if matches!(effect, ArgEffect::MutableBorrow | ArgEffect::MayConsume)
        && let HirExpressionKind::Load(place) = &argument.kind
    {
        let mut read_env = SharedReadEnv {
            context: input.context,
            layout: input.layout,
            state: input.state,
            tracker: input.tracker,
            location: argument_location.clone(),
            current_order: input.current_order,
            stats: input.stats,
            value_fact_buffer: input.value_fact_buffer,
        };
        record_shared_reads_in_place_indices(
            &mut read_env,
            place,
            argument_location.clone(),
            arg_roots,
        )?;
        let place_roots = roots_for_place(
            input.layout,
            input.state,
            place,
            argument_location.clone(),
            &input.context.diagnostics,
        )?;
        arg_roots.union_with(&place_roots);
        return Ok(());
    }

    let mut read_env = SharedReadEnv {
        context: input.context,
        layout: input.layout,
        state: input.state,
        tracker: input.tracker,
        location: argument_location.clone(),
        current_order: input.current_order,
        stats: input.stats,
        value_fact_buffer: input.value_fact_buffer,
    };
    record_shared_reads_in_expression(
        &mut read_env,
        argument,
        argument_location.clone(),
        arg_roots,
    )
}

fn transfer_call_argument_access(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    argument: &HirExpression,
    effect: ArgEffect,
    argument_location: SourceLocation,
) -> Result<(), BorrowCheckError> {
    match effect {
        ArgEffect::SharedBorrow => Ok(()),
        ArgEffect::MutableBorrow => {
            let mutable_roots = mutable_argument_roots(
                input.layout,
                input.state,
                argument,
                argument_location.clone(),
                &input.context.diagnostics,
            )?;
            check_call_mutable_borrow(input, &mutable_roots, argument_location)?;
            input.value_fact_buffer.record(
                argument.id,
                ValueAccessClassification::MutableArgument,
                &mutable_roots,
            );
            Ok(())
        }
        ArgEffect::MayConsume => {
            let mutable_roots = mutable_argument_roots(
                input.layout,
                input.state,
                argument,
                argument_location.clone(),
                &input.context.diagnostics,
            )?;
            check_call_may_consume(input, &mutable_roots, argument_location)?;
            input.value_fact_buffer.record(
                argument.id,
                ValueAccessClassification::MutableArgument,
                &mutable_roots,
            );
            Ok(())
        }
    }
}

fn check_call_mutable_borrow(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    mutable_roots: &RootSet,
    location: SourceLocation,
) -> Result<(), BorrowCheckError> {
    if mutable_roots.is_empty() {
        return Ok(());
    }

    let mut check = AccessCheckContext {
        context: input.context,
        layout: input.layout,
        state: input.state,
        tracker: input.tracker,
        location,
        stats: input.stats,
        actor_index_hint: None,
        current_order: input.current_order,
    };
    check_mutable_access(
        &mut check,
        mutable_roots,
        MutableAccessPolicy {
            allow_prior_shared: false,
            require_root_mutable: true,
            strict_move_exclusivity: false,
        },
    )
}

fn check_call_may_consume(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    mutable_roots: &RootSet,
    location: SourceLocation,
) -> Result<(), BorrowCheckError> {
    if mutable_roots.is_empty() {
        return Ok(());
    }

    match classify_move_decision(
        input.layout,
        input.block_id,
        mutable_roots,
        input.current_order,
    ) {
        MoveDecision::Borrow => check_call_mutable_borrow(input, mutable_roots, location),
        MoveDecision::Move => {
            let mut check = AccessCheckContext {
                context: input.context,
                layout: input.layout,
                state: input.state,
                tracker: input.tracker,
                location,
                stats: input.stats,
                actor_index_hint: None,
                current_order: input.current_order,
            };
            check_mutable_access(
                &mut check,
                mutable_roots,
                MutableAccessPolicy {
                    allow_prior_shared: false,
                    require_root_mutable: false,
                    strict_move_exclusivity: true,
                },
            )?;

            for root_index in mutable_roots.iter_ones() {
                input.state.invalidate_root(root_index);
            }
            Ok(())
        }
        MoveDecision::Inconsistent(root_index) => Err(input
            .context
            .diagnostics
            .invalid_access_after_possible_ownership_transfer(
                input
                    .context
                    .diagnostics
                    .local_place(input.layout.local_ids[root_index]),
                location,
            )),
    }
}

fn transfer_call_result_alias(
    input: &mut CallTransferContext<'_, '_, '_, '_, '_, '_>,
    result: Option<LocalId>,
    return_alias: CallResultAlias,
    arg_roots: &[RootSet],
) -> Result<(), BorrowCheckError> {
    let Some(result_local) = result else {
        return Ok(());
    };

    let Some(local_index) = input.layout.index_of(result_local) else {
        return Err(input.context.diagnostics.internal_error(
            format!(
                "Call result local '{}' is not in the active function layout",
                input.context.diagnostics.local_name(result_local)
            ),
            input.location.clone(),
        ));
    };

    let alias_roots = match return_alias {
        CallResultAlias::Fresh => None,
        CallResultAlias::AliasArgs(ref arg_indices) => {
            let mut roots = RootSet::empty(input.layout.local_count());
            for arg_index in arg_indices {
                let Some(arg_root_set) = arg_roots.get(*arg_index) else {
                    return Err(input.context.diagnostics.internal_error(
                        format!(
                            "Borrow checker found out-of-range return-alias index {arg_index} at call site"
                        ),
                        input.location.clone(),
                    ));
                };
                roots.union_with(arg_root_set);
            }
            Some(roots)
        }
        CallResultAlias::Unknown => {
            let mut roots = RootSet::empty(input.layout.local_count());
            for arg_root_set in arg_roots {
                roots.union_with(arg_root_set);
            }
            Some(roots)
        }
    };

    let new_local_state = match alias_roots {
        Some(roots) if !roots.is_empty() => LocalState::alias(roots),
        _ => LocalState::slot(input.layout.local_count()),
    };
    input.state.update_local_state(local_index, new_local_state);
    Ok(())
}

fn map_argument_effect(op: HirMapOp, arg_index: usize) -> ArgEffect {
    if matches!(op, HirMapOp::Set) && matches!(arg_index, 0 | 1) {
        ArgEffect::MayConsume
    } else {
        ArgEffect::SharedBorrow
    }
}

fn map_result_alias(op: HirMapOp) -> CallResultAlias {
    if matches!(op, HirMapOp::Get) {
        CallResultAlias::AliasArgs(vec![0])
    } else {
        CallResultAlias::Fresh
    }
}

fn reactive_assignment_invalidations(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    statement_id: HirNodeId,
    target: &HirPlace,
    location: SourceLocation,
) -> Result<Vec<ReactiveInvalidationFact>, BorrowCheckError> {
    match target {
        HirPlace::Local(local_id) => {
            let Some(source) = context.diagnostics.reactive_source_id_for_local(*local_id) else {
                return Ok(Vec::new());
            };

            let Some(local_index) = layout.index_of(*local_id) else {
                return Err(context.diagnostics.internal_error(
                    format!(
                        "Borrow checker could not resolve reactive assignment local '{local_id}' in the current function"
                    ),
                    location,
                ));
            };

            // Declaration initialization also lowers as an assignment. It creates the stable
            // source storage, but it does not invalidate an already-live source.
            if state.local_state(local_index).mode.is_definitely_uninit() {
                return Ok(Vec::new());
            }

            Ok(vec![ReactiveInvalidationFact {
                statement_id,
                source,
                kind: ReactiveInvalidationKind::Assignment,
                location,
            }])
        }

        HirPlace::Field { .. } | HirPlace::Index { .. } => {
            let roots = roots_for_place(
                layout,
                state,
                target,
                location.clone(),
                &context.diagnostics,
            )?;
            let kind = match target {
                HirPlace::Field { .. } => ReactivePlaceWriteKind::Field,
                HirPlace::Index { .. } => ReactivePlaceWriteKind::Index,
                HirPlace::Local(_) => return Ok(Vec::new()),
            };
            Ok(invalidations_for_roots(
                context,
                layout,
                &roots,
                statement_id,
                ReactiveInvalidationKind::PlaceWrite(kind),
                location,
            ))
        }
    }
}

fn reactive_map_mutation_invalidations(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    statement_id: HirNodeId,
    op: HirMapOp,
    receiver: &HirExpression,
    location: SourceLocation,
) -> Result<Vec<ReactiveInvalidationFact>, BorrowCheckError> {
    if !op.requires_mutable_receiver() {
        return Ok(Vec::new());
    }

    let roots = mutable_argument_roots(
        layout,
        state,
        receiver,
        location.clone(),
        &context.diagnostics,
    )?;
    Ok(invalidations_for_roots(
        context,
        layout,
        &roots,
        statement_id,
        ReactiveInvalidationKind::MapMutation(op),
        location,
    ))
}

fn reactive_mutable_call_invalidations(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    statement_id: HirNodeId,
    target: &CallTarget,
    args: &[CallArgumentTransfer<'_>],
    location: SourceLocation,
) -> Result<Vec<ReactiveInvalidationFact>, BorrowCheckError> {
    let mut invalidations = Vec::new();

    for (argument_index, arg) in args.iter().enumerate() {
        if matches!(arg.effect, ArgEffect::SharedBorrow) {
            continue;
        }

        let argument_location = context
            .diagnostics
            .value_error_location(arg.argument.id, location.clone());
        let roots = mutable_argument_roots(
            layout,
            state,
            arg.argument,
            argument_location.clone(),
            &context.diagnostics,
        )?;
        invalidations.extend(invalidations_for_roots(
            context,
            layout,
            &roots,
            statement_id,
            ReactiveInvalidationKind::MutableCallArgument {
                target: target.clone(),
                argument_index,
            },
            argument_location,
        ));
    }

    Ok(invalidations)
}

fn invalidations_for_roots(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    roots: &RootSet,
    statement_id: HirNodeId,
    kind: ReactiveInvalidationKind,
    location: SourceLocation,
) -> Vec<ReactiveInvalidationFact> {
    let mut sources = roots
        .iter_ones()
        .filter_map(|root_index| {
            context
                .diagnostics
                .reactive_source_id_for_local(layout.local_ids[root_index])
        })
        .collect::<Vec<_>>();
    sources.sort_by_key(|source| source.0);
    sources.dedup();

    sources
        .into_iter()
        .map(|source| ReactiveInvalidationFact {
            statement_id,
            source,
            kind: kind.clone(),
            location: location.clone(),
        })
        .collect()
}
