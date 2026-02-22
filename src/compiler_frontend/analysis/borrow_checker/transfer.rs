use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, LocalState, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{AccessKind, LocalMode};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirPlace,
    HirStatement, HirStatementKind, HirTerminator, OptionVariant,
};
use crate::compiler_frontend::hir::hir_nodes::{HirBlock, HirModule};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_borrow_checker_error;
use rustc_hash::FxHashMap;

pub(super) struct BorrowTransferContext<'a> {
    pub module: &'a HirModule,
    pub string_table: &'a StringTable,
    pub function_by_path: &'a FxHashMap<InternedPath, FunctionId>,
    pub function_param_mutability: &'a FxHashMap<FunctionId, Vec<bool>>,
    pub diagnostics: BorrowDiagnostics<'a>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockTransferStats {
    pub statements_analyzed: usize,
    pub terminators_analyzed: usize,
    pub conflicts_checked: usize,
    pub mutable_call_sites: usize,
}

pub(super) fn transfer_block(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    block: &HirBlock,
    state: &mut BorrowState,
) -> Result<BlockTransferStats, CompilerError> {
    let mut stats = BlockTransferStats::default();

    for statement in &block.statements {
        transfer_statement(context, layout, state, statement, &mut stats)?;
        stats.statements_analyzed += 1;
    }

    transfer_terminator(
        context,
        layout,
        state,
        block.id,
        &block.terminator,
        &mut stats,
    )?;
    stats.terminators_analyzed += 1;

    Ok(stats)
}

fn transfer_statement(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    statement: &HirStatement,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    let mut tracker = StatementAccessTracker::new(layout.local_count());

    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            let location = context.diagnostics.statement_error_location(statement);

            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                target,
                &mut tracker,
                location.clone(),
                stats,
            )?;
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                value,
                &mut tracker,
                location.clone(),
                stats,
            )?;

            transfer_assign_target(
                context,
                layout,
                state,
                target,
                value,
                &mut tracker,
                location,
                stats,
            )?;
        }

        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            let location = context.diagnostics.statement_error_location(statement);
            let mutability =
                resolve_call_arg_mutability(context, target, args.len(), location.clone())?;
            if mutability.iter().any(|is_mutable| *is_mutable) {
                stats.mutable_call_sites += 1;
            }

            for (arg_index, argument) in args.iter().enumerate() {
                if mutability[arg_index] {
                    record_shared_reads_for_mutable_argument(
                        context,
                        layout,
                        state,
                        argument,
                        &mut tracker,
                        location.clone(),
                        stats,
                    )?;

                    let mutable_roots =
                        mutable_argument_roots(layout, state, argument, location.clone())?;
                    if !mutable_roots.is_empty() {
                        check_mutable_access(
                            context,
                            layout,
                            state,
                            &mutable_roots,
                            None,
                            &mut tracker,
                            location.clone(),
                            stats,
                        )?;
                    }
                } else {
                    record_shared_reads_in_expression(
                        context,
                        layout,
                        state,
                        argument,
                        &mut tracker,
                        location.clone(),
                        stats,
                    )?;
                }
            }

            if let Some(result_local) = result {
                let Some(local_index) = layout.index_of(*result_local) else {
                    return_borrow_checker_error!(
                        format!(
                            "Call result local '{}' is not in the active function layout",
                            context.diagnostics.local_name(*result_local)
                        ),
                        location,
                        {
                            CompilationStage => "Borrow Checking",
                        }
                    );
                };

                let local_state = state.local_state(local_index).clone();
                if local_state.mode.is_definitely_uninit() {
                    state.update_local_state(local_index, LocalState::slot(layout.local_count()));
                }
            }
        }

        HirStatementKind::Expr(expression) => {
            let location = context.diagnostics.statement_error_location(statement);
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                expression,
                &mut tracker,
                location,
                stats,
            )?;
        }

        HirStatementKind::Drop(_local) => {
            // Drop sites are ownership/runtime-lowering concerns. Borrow enforcement
            // in this phase only validates aliasing and mutable exclusivity.
        }
    }

    Ok(())
}

fn record_shared_reads_for_mutable_argument(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    if let HirExpressionKind::Load(place) = &expression.kind {
        return record_shared_reads_in_place_indices(
            context, layout, state, place, tracker, location, stats,
        );
    }

    record_shared_reads_in_expression(context, layout, state, expression, tracker, location, stats)
}

fn transfer_terminator(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    block_id: BlockId,
    terminator: &HirTerminator,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    let mut tracker = StatementAccessTracker::new(layout.local_count());
    let location = context
        .diagnostics
        .terminator_error_location(block_id, terminator);

    match terminator {
        HirTerminator::Jump { args, .. } => {
            for local in args {
                let Some(local_index) = layout.index_of(*local) else {
                    return_borrow_checker_error!(
                        format!(
                            "Jump argument local '{}' is not in the active function layout",
                            context.diagnostics.local_name(*local)
                        ),
                        location.clone(),
                        {
                            CompilationStage => "Borrow Checking",
                        }
                    );
                };

                let roots = state.effective_roots(local_index);
                check_shared_access(
                    context,
                    layout,
                    &roots,
                    &mut tracker,
                    location.clone(),
                    stats,
                )?;
            }
        }

        HirTerminator::If { condition, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                condition,
                &mut tracker,
                location,
                stats,
            )?;
        }

        HirTerminator::Match { scrutinee, arms } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                scrutinee,
                &mut tracker,
                location.clone(),
                stats,
            )?;

            for arm in arms {
                record_shared_reads_in_pattern(
                    context,
                    layout,
                    state,
                    arm,
                    &mut tracker,
                    location.clone(),
                    stats,
                )?;
            }
        }

        HirTerminator::Return(value) => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                value,
                &mut tracker,
                location,
                stats,
            )?;
        }

        HirTerminator::Panic { message } => {
            if let Some(message) = message {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    message,
                    &mut tracker,
                    location,
                    stats,
                )?;
            }
        }

        HirTerminator::Loop { .. }
        | HirTerminator::Break { .. }
        | HirTerminator::Continue { .. } => {}
    }

    Ok(())
}

fn record_shared_reads_in_pattern(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    arm: &HirMatchArm,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    if let HirPattern::Literal(expression) = &arm.pattern {
        record_shared_reads_in_expression(
            context,
            layout,
            state,
            expression,
            tracker,
            location.clone(),
            stats,
        )?;
    }

    if let Some(guard) = &arm.guard {
        record_shared_reads_in_expression(context, layout, state, guard, tracker, location, stats)?;
    }

    Ok(())
}

fn resolve_call_arg_mutability(
    context: &BorrowTransferContext<'_>,
    target: &CallTarget,
    arg_len: usize,
    location: ErrorLocation,
) -> Result<Vec<bool>, CompilerError> {
    match target {
        CallTarget::HostFunction(_) => Ok(vec![false; arg_len]),

        CallTarget::UserFunction(path) => {
            let Some(function_id) = context.function_by_path.get(path).copied() else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve user call target '{}'",
                        context.diagnostics.path_name(path)
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure the called function is declared in the module before use",
                    }
                );
            };

            let Some(param_mutability) = context.function_param_mutability.get(&function_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker is missing parameter mutability metadata for function '{}'",
                        context.diagnostics.function_name(function_id)
                    ),
                    context.diagnostics.function_error_location(function_id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            if param_mutability.len() != arg_len {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker found argument count mismatch for function '{}': expected {}, got {}",
                        context.diagnostics.function_name(function_id),
                        param_mutability.len(),
                        arg_len
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure call argument count matches the function signature",
                    }
                );
            }

            Ok(param_mutability.clone())
        }
    }
}

fn transfer_assign_target(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    target: &HirPlace,
    value: &HirExpression,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    match target {
        HirPlace::Local(local_id) => {
            let Some(local_index) = layout.index_of(*local_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Assignment target local '{}' is not in the active function layout",
                        context.diagnostics.local_name(*local_id)
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            let local_state = state.local_state(local_index).clone();
            let rhs_alias_roots =
                direct_place_roots_from_expression(layout, state, value, location.clone())?;

            if local_state.mode.is_definitely_uninit() {
                apply_slot_rebinding(state, layout.local_count(), local_index, rhs_alias_roots);
                return Ok(());
            }

            let mut write_roots = RootSet::empty(layout.local_count());
            if local_state.mode.contains(LocalMode::SLOT) {
                write_roots.insert(local_index);
            }
            if local_state.mode.contains(LocalMode::ALIAS) {
                write_roots.union_with(&local_state.alias_roots);
            }

            check_mutable_access(
                context,
                layout,
                state,
                &write_roots,
                Some(local_index),
                tracker,
                location.clone(),
                stats,
            )?;

            match (
                local_state.mode.contains(LocalMode::SLOT),
                local_state.mode.contains(LocalMode::ALIAS),
            ) {
                (false, true) => {
                    // Alias-view writes through to referent and does not rebind.
                }

                (true, false) => {
                    apply_slot_rebinding(state, layout.local_count(), local_index, rhs_alias_roots);
                }

                (true, true) => {
                    let mut alias_roots = local_state.alias_roots;
                    if let Some(rhs_roots) = rhs_alias_roots {
                        alias_roots.union_with(&rhs_roots);
                    }

                    state.update_local_state(
                        local_index,
                        LocalState {
                            mode: LocalMode::SLOT.union(LocalMode::ALIAS),
                            alias_roots,
                        },
                    );
                }

                (false, false) => {
                    state.update_local_state(local_index, LocalState::slot(layout.local_count()));
                }
            }
        }

        _ => {
            let roots = roots_for_place(layout, state, target, location.clone())?;
            check_mutable_access(
                context, layout, state, &roots, None, tracker, location, stats,
            )?;
        }
    }

    Ok(())
}

fn apply_slot_rebinding(
    state: &mut BorrowState,
    local_count: usize,
    local_index: usize,
    rhs_alias_roots: Option<RootSet>,
) {
    match rhs_alias_roots {
        Some(roots) => state.update_local_state(local_index, LocalState::alias(roots)),
        None => state.update_local_state(local_index, LocalState::slot(local_count)),
    }
}

fn check_shared_access(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    roots: &RootSet,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    for root_index in roots.iter_ones() {
        stats.conflicts_checked += 1;

        if let Some(existing) = tracker.conflict(root_index, AccessKind::Shared) {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);

            return_borrow_checker_error!(
                format!(
                    "Cannot read '{}' as shared after a mutable access in the same evaluation sequence ({:?} -> Shared)",
                    root_name,
                    existing
                ),
                location,
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Shared",
                    PrimarySuggestion => "Split the expression into separate statements to avoid overlapping access modes",
                }
            );
        }

        tracker.record(root_index, AccessKind::Shared);
    }

    Ok(())
}

fn check_mutable_access(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    roots: &RootSet,
    actor_index_hint: Option<usize>,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    for root_index in roots.iter_ones() {
        stats.conflicts_checked += 1;

        if let Some(existing) = tracker.conflict(root_index, AccessKind::Mutable) {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);

            return_borrow_checker_error!(
                format!(
                    "Cannot mutably access '{}' due to overlapping {:?} access in the same evaluation sequence",
                    root_name,
                    existing
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    PrimarySuggestion => "Split mutable and shared accesses into separate statements",
                }
            );
        }

        if !layout.local_mutable[root_index] {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);
            return_borrow_checker_error!(
                format!(
                    "Cannot mutably access immutable local '{}'",
                    root_name
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    PrimarySuggestion => "Declare the variable as mutable with '~=' before mutating it",
                }
            );
        }

        let alias_count = state.alias_count_for_root(root_index);
        if alias_count > 1 {
            let actor_index = actor_index_hint.unwrap_or(root_index);
            let actor_name = context
                .diagnostics
                .local_name(layout.local_ids[actor_index]);
            let conflicting_local = context
                .diagnostics
                .conflicting_local_for_root(layout, state, actor_index, root_index)
                .map(|local| context.diagnostics.local_name(local))
                .unwrap_or_else(|| String::from("<unknown>"));

            return_borrow_checker_error!(
                format!(
                    "Cannot mutably access '{}' because '{}' may alias the same value",
                    actor_name, conflicting_local
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    LifetimeHint => "Mutable access requires exclusive aliasing",
                }
            );
        }

        tracker.record(root_index, AccessKind::Mutable);
    }

    Ok(())
}

fn mutable_argument_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    location: ErrorLocation,
) -> Result<RootSet, CompilerError> {
    if let HirExpressionKind::Load(place) = &expression.kind {
        return roots_for_place(layout, state, place, location);
    }

    let mut roots = RootSet::empty(layout.local_count());
    collect_expression_roots(layout, state, expression, &mut roots, location)?;
    Ok(roots)
}

fn direct_place_roots_from_expression(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    location: ErrorLocation,
) -> Result<Option<RootSet>, CompilerError> {
    if let HirExpressionKind::Load(place) = &expression.kind {
        return Ok(Some(roots_for_place(layout, state, place, location)?));
    }

    Ok(None)
}

fn roots_for_place(
    layout: &FunctionLayout,
    state: &BorrowState,
    place: &HirPlace,
    location: ErrorLocation,
) -> Result<RootSet, CompilerError> {
    match place {
        HirPlace::Local(local_id) => {
            let Some(local_index) = layout.index_of(*local_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve place local '{}' in the current function",
                        local_id
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            Ok(state.effective_roots(local_index))
        }

        HirPlace::Field { base, .. } => roots_for_place(layout, state, base, location),

        HirPlace::Index { base, .. } => roots_for_place(layout, state, base, location),
    }
}

fn record_shared_reads_in_place_indices(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    place: &HirPlace,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    match place {
        HirPlace::Local(_) => Ok(()),

        HirPlace::Field { base, .. } => record_shared_reads_in_place_indices(
            context, layout, state, base, tracker, location, stats,
        ),

        HirPlace::Index { base, index } => {
            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                base,
                tracker,
                location.clone(),
                stats,
            )?;

            record_shared_reads_in_expression(
                context, layout, state, index, tracker, location, stats,
            )
        }
    }
}

fn record_shared_reads_in_expression(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}

        HirExpressionKind::Load(place) => {
            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                place,
                tracker,
                location.clone(),
                stats,
            )?;

            let roots = roots_for_place(layout, state, place, location.clone())?;
            check_shared_access(context, layout, &roots, tracker, location, stats)?;
        }

        HirExpressionKind::BinOp { left, right, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                left,
                tracker,
                location.clone(),
                stats,
            )?;
            record_shared_reads_in_expression(
                context, layout, state, right, tracker, location, stats,
            )?;
        }

        HirExpressionKind::UnaryOp { operand, .. } => {
            record_shared_reads_in_expression(
                context, layout, state, operand, tracker, location, stats,
            )?;
        }

        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    value,
                    tracker,
                    location.clone(),
                    stats,
                )?;
            }
        }

        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    element,
                    tracker,
                    location.clone(),
                    stats,
                )?;
            }
        }

        HirExpressionKind::Range { start, end } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                start,
                tracker,
                location.clone(),
                stats,
            )?;
            record_shared_reads_in_expression(
                context, layout, state, end, tracker, location, stats,
            )?;
        }

        HirExpressionKind::OptionConstruct { variant, value } => {
            if matches!(variant, OptionVariant::Some) {
                if let Some(inner) = value {
                    record_shared_reads_in_expression(
                        context, layout, state, inner, tracker, location, stats,
                    )?;
                }
            }
        }

        HirExpressionKind::ResultConstruct { value, .. } => {
            record_shared_reads_in_expression(
                context, layout, state, value, tracker, location, stats,
            )?;
        }
    }

    Ok(())
}

fn collect_expression_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    out: &mut RootSet,
    location: ErrorLocation,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Load(place) => {
            let roots = roots_for_place(layout, state, place, location.clone())?;
            out.union_with(&roots);

            if let HirPlace::Index { index, .. } = place {
                collect_expression_roots(layout, state, index, out, location)?;
            }
        }

        HirExpressionKind::BinOp { left, right, .. } => {
            collect_expression_roots(layout, state, left, out, location.clone())?;
            collect_expression_roots(layout, state, right, out, location)?;
        }

        HirExpressionKind::UnaryOp { operand, .. } => {
            collect_expression_roots(layout, state, operand, out, location)?;
        }

        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                collect_expression_roots(layout, state, value, out, location.clone())?;
            }
        }

        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                collect_expression_roots(layout, state, element, out, location.clone())?;
            }
        }

        HirExpressionKind::Range { start, end } => {
            collect_expression_roots(layout, state, start, out, location.clone())?;
            collect_expression_roots(layout, state, end, out, location)?;
        }

        HirExpressionKind::OptionConstruct { value, .. } => {
            if let Some(inner) = value {
                collect_expression_roots(layout, state, inner, out, location)?;
            }
        }

        HirExpressionKind::ResultConstruct { value, .. } => {
            collect_expression_roots(layout, state, value, out, location)?;
        }

        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct StatementAccessTracker {
    root_access: Vec<Option<AccessKind>>,
}

impl StatementAccessTracker {
    fn new(root_count: usize) -> Self {
        Self {
            root_access: vec![None; root_count],
        }
    }

    fn conflict(&self, root_index: usize, new_access: AccessKind) -> Option<AccessKind> {
        let existing = self.root_access[root_index]?;

        match (existing, new_access) {
            (AccessKind::Shared, AccessKind::Shared) => None,
            (AccessKind::Shared, AccessKind::Mutable)
            | (AccessKind::Mutable, AccessKind::Shared)
            | (AccessKind::Mutable, AccessKind::Mutable) => Some(existing),
        }
    }

    fn record(&mut self, root_index: usize, access: AccessKind) {
        let entry = &mut self.root_access[root_index];
        match (*entry, access) {
            (Some(AccessKind::Mutable), _) => {}
            (_, AccessKind::Mutable) => *entry = Some(AccessKind::Mutable),
            (None, AccessKind::Shared) => *entry = Some(AccessKind::Shared),
            (Some(AccessKind::Shared), AccessKind::Shared) => {}
        }
    }
}
