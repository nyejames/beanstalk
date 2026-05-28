//! Statement-level borrow transfer rules.
//!
//! WHAT: applies borrow effects for HIR statements and records statement/value facts.
//! WHY: keeping statement transfer separate from traversal helpers makes the block transfer
//! entrypoint easier to inspect without changing the borrow-analysis model.

use super::*;

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
    let conflicts_before = stats.conflicts_checked;
    let statement_order = layout.statement_order_or_unknown(statement.id);

    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            let location = context.diagnostics.statement_error_location(statement);

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
        }

        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            let location = context.diagnostics.statement_error_location(statement);
            let semantics = resolve_call_semantics(context, target, args.len(), location.clone())?;
            if semantics
                .arg_effects
                .iter()
                .any(|effect| !matches!(effect, ArgEffect::SharedBorrow))
            {
                stats.mutable_call_sites += 1;
            }

            let mut arg_roots = vec![RootSet::empty(layout.local_count()); args.len()];

            for (arg_index, argument) in args.iter().enumerate() {
                let argument_location = context
                    .diagnostics
                    .value_error_location(argument.id, location.clone());
                let arg_effect = semantics.arg_effects[arg_index];

                if matches!(arg_effect, ArgEffect::MutableBorrow | ArgEffect::MayConsume) {
                    // For mutable arguments, the argument root itself should be treated as
                    // mutable access, not an initial shared load. We still record any shared
                    // reads needed to evaluate projections (for example index expressions).
                    match &argument.kind {
                        HirExpressionKind::Load(place) => {
                            let mut read_env = SharedReadEnv {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                current_order: statement_order,
                                stats,
                                value_fact_buffer,
                            };
                            let mut arg_root_set = RootSet::empty(layout.local_count());
                            record_shared_reads_in_place_indices(
                                &mut read_env,
                                place,
                                argument_location.clone(),
                                &mut arg_root_set,
                            )?;
                            let place_roots = roots_for_place(
                                layout,
                                state,
                                place,
                                argument_location.clone(),
                                &context.diagnostics,
                            )?;
                            arg_root_set.union_with(&place_roots);
                            arg_roots[arg_index] = arg_root_set;
                        }
                        _ => {
                            let mut arg_root_set = RootSet::empty(layout.local_count());
                            let mut read_env = SharedReadEnv {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                current_order: statement_order,
                                stats,
                                value_fact_buffer,
                            };
                            record_shared_reads_in_expression(
                                &mut read_env,
                                argument,
                                argument_location.clone(),
                                &mut arg_root_set,
                            )?;
                            arg_roots[arg_index] = arg_root_set;
                        }
                    }
                } else {
                    let mut arg_root_set = RootSet::empty(layout.local_count());
                    let mut read_env = SharedReadEnv {
                        context,
                        layout,
                        state,
                        tracker: &mut tracker,
                        location: argument_location.clone(),
                        current_order: statement_order,
                        stats,
                        value_fact_buffer,
                    };
                    record_shared_reads_in_expression(
                        &mut read_env,
                        argument,
                        argument_location.clone(),
                        &mut arg_root_set,
                    )?;
                    arg_roots[arg_index] = arg_root_set;
                }

                match arg_effect {
                    ArgEffect::SharedBorrow => {}
                    ArgEffect::MutableBorrow => {
                        let mutable_roots = mutable_argument_roots(
                            layout,
                            state,
                            argument,
                            argument_location.clone(),
                            &context.diagnostics,
                        )?;
                        if !mutable_roots.is_empty() {
                            let mut check = AccessCheckContext {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                stats,
                                actor_index_hint: None,
                                current_order: statement_order,
                            };
                            check_mutable_access(
                                &mut check,
                                &mutable_roots,
                                MutableAccessPolicy {
                                    allow_prior_shared: false,
                                    require_root_mutable: true,
                                    strict_move_exclusivity: false,
                                },
                            )?;
                        }

                        value_fact_buffer.record(
                            argument.id,
                            ValueAccessClassification::MutableArgument,
                            &mutable_roots,
                        );
                    }
                    ArgEffect::MayConsume => {
                        let mutable_roots = mutable_argument_roots(
                            layout,
                            state,
                            argument,
                            argument_location.clone(),
                            &context.diagnostics,
                        )?;
                        if !mutable_roots.is_empty() {
                            // WHAT: choose borrow vs move at the call site from future-use facts.
                            // WHY: user mutable params can either borrow or consume through one ABI.
                            match classify_move_decision(
                                layout,
                                block_id,
                                &mutable_roots,
                                statement_order,
                            ) {
                                MoveDecision::Borrow => {
                                    let mut check = AccessCheckContext {
                                        context,
                                        layout,
                                        state,
                                        tracker: &mut tracker,
                                        location: argument_location.clone(),
                                        stats,
                                        actor_index_hint: None,
                                        current_order: statement_order,
                                    };
                                    check_mutable_access(
                                        &mut check,
                                        &mutable_roots,
                                        MutableAccessPolicy {
                                            allow_prior_shared: false,
                                            require_root_mutable: true,
                                            strict_move_exclusivity: false,
                                        },
                                    )?;
                                }
                                MoveDecision::Move => {
                                    let mut check = AccessCheckContext {
                                        context,
                                        layout,
                                        state,
                                        tracker: &mut tracker,
                                        location: argument_location.clone(),
                                        stats,
                                        actor_index_hint: None,
                                        current_order: statement_order,
                                    };
                                    check_mutable_access(
                                        &mut check,
                                        &mutable_roots,
                                        MutableAccessPolicy {
                                            allow_prior_shared: false,
                                            require_root_mutable: false,
                                            strict_move_exclusivity: true,
                                        },
                                    )?;

                                    for root_index in mutable_roots.iter_ones() {
                                        state.invalidate_root(root_index);
                                    }
                                }
                                MoveDecision::Inconsistent(root_index) => {
                                    return Err(context
                                        .diagnostics
                                        .invalid_access_after_possible_ownership_transfer(
                                            context
                                                .diagnostics
                                                .local_place(layout.local_ids[root_index]),
                                            argument_location.clone(),
                                        ));
                                }
                            }
                        }

                        value_fact_buffer.record(
                            argument.id,
                            ValueAccessClassification::MutableArgument,
                            &mutable_roots,
                        );
                    }
                }
            }

            if let Some(result_local) = result {
                let Some(local_index) = layout.index_of(*result_local) else {
                    return Err(context.diagnostics.internal_error(
                        format!(
                            "Call result local '{}' is not in the active function layout",
                            context.diagnostics.local_name(*result_local)
                        ),
                        location,
                    ));
                };

                let alias_roots = match semantics.return_alias {
                    CallResultAlias::Fresh => None,
                    CallResultAlias::AliasArgs(ref arg_indices) => {
                        let mut roots = RootSet::empty(layout.local_count());
                        for arg_index in arg_indices {
                            let Some(arg_root_set) = arg_roots.get(*arg_index) else {
                                return Err(context.diagnostics.internal_error(
                                    format!(
                                        "Borrow checker found out-of-range return-alias index {} at call site",
                                        arg_index
                                    ),
                                    location.clone(),
                                ));
                            };
                            roots.union_with(arg_root_set);
                        }
                        Some(roots)
                    }
                    CallResultAlias::Unknown => {
                        let mut roots = RootSet::empty(layout.local_count());
                        for arg_root_set in &arg_roots {
                            roots.union_with(arg_root_set);
                        }
                        Some(roots)
                    }
                };

                let new_local_state = match alias_roots {
                    Some(roots) if !roots.is_empty() => LocalState::alias(roots),
                    _ => LocalState::slot(layout.local_count()),
                };
                state.update_local_state(local_index, new_local_state);
            }
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

    let statement_fact = StatementBorrowFact {
        shared_roots: roots_to_local_ids(layout, &tracker.shared_roots),
        mutable_roots: roots_to_local_ids(layout, &tracker.mutable_roots),
        conflicts_checked: stats.conflicts_checked - conflicts_before,
    };
    stats.statement_facts.push((statement.id, statement_fact));

    Ok(())
}
