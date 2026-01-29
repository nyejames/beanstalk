//! Lightweight HIR analysis used by the JS codegen backend.
//!
//! The JS emitter lowers the CFG into a block-dispatch state machine, so it needs
//! a pre-pass to determine reachable blocks, locals that must be declared up-front,
//! and which runtime helpers (template result map, loop state map) are required.
//! Loop metadata is also collected so `break`/`continue` can be rewritten to the
//! correct dispatch target when loops carry iteration state.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::hir::nodes::{
    BlockId, HirBlock, HirExpr, HirExprKind, HirKind, HirPlace, HirStmt, HirTerminator,
};
use crate::compiler::string_interning::InternedString;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Default)]
pub struct LoopMetadata {
    pub header_by_body: HashMap<BlockId, BlockId>,
    pub header_by_exit: HashMap<BlockId, BlockId>,
}

#[derive(Debug, Clone)]
pub struct FunctionAnalysis {
    pub block_ids: Vec<BlockId>,
    pub locals: Vec<InternedString>,
    pub uses_template_results: bool,
    pub uses_loop_state: bool,
    pub loop_metadata: LoopMetadata,
}

pub fn analyze_function(
    entry: BlockId,
    blocks: &[HirBlock],
    params: &[InternedString],
) -> Result<FunctionAnalysis, CompilerError> {
    let block_ids = collect_reachable_blocks(entry, blocks)?;
    let mut locals = HashSet::new();
    let mut uses_template_results = false;
    let mut uses_loop_state = false;
    let params_set: HashSet<InternedString> = params.iter().copied().collect();
    let mut loop_metadata = LoopMetadata::default();

    for block_id in &block_ids {
        let block = get_block(blocks, *block_id)?;
        for node in &block.nodes {
            match &node.kind {
                HirKind::Stmt(stmt) => match stmt {
                    HirStmt::Assign { target, value, .. } => {
                        if let HirPlace::Var(name) = target {
                            if !params_set.contains(name) {
                                locals.insert(*name);
                            }
                        }
                        scan_expr(value, &mut uses_template_results);
                    }
                    HirStmt::Call { args, .. } | HirStmt::HostCall { args, .. } => {
                        for arg in args {
                            scan_expr(arg, &mut uses_template_results);
                        }
                    }
                    HirStmt::RuntimeTemplateCall { captures, .. } => {
                        // Template call results are stored in a map and reused by HeapString.
                        uses_template_results = true;
                        for capture in captures {
                            scan_expr(capture, &mut uses_template_results);
                        }
                    }
                    HirStmt::ExprStmt(expr) => {
                        scan_expr(expr, &mut uses_template_results);
                    }
                    HirStmt::PossibleDrop(_) => {}
                    HirStmt::TemplateFn { .. }
                    | HirStmt::FunctionDef { .. }
                    | HirStmt::StructDef { .. } => {}
                },
                HirKind::Terminator(term) => {
                    scan_terminator(term, &mut locals, &mut uses_template_results, &params_set);
                    if let HirTerminator::Loop {
                        label,
                        body,
                        iterator,
                        ..
                    } = term
                    {
                        // Map loop bodies/exits back to the header so JS can rewrite targets.
                        loop_metadata.header_by_body.insert(*body, *block_id);
                        loop_metadata.header_by_exit.insert(*label, *block_id);
                        if iterator.is_some() {
                            // Iterator loops need persistent per-header state in JS.
                            uses_loop_state = true;
                        }
                    }
                }
            }
        }
    }

    let mut locals_vec: Vec<InternedString> = locals.into_iter().collect();
    locals_vec.sort_by(|left, right| left.as_u32().cmp(&right.as_u32()));

    Ok(FunctionAnalysis {
        block_ids,
        locals: locals_vec,
        uses_template_results,
        uses_loop_state,
        loop_metadata,
    })
}

fn collect_reachable_blocks(
    entry: BlockId,
    blocks: &[HirBlock],
) -> Result<Vec<BlockId>, CompilerError> {
    get_block(blocks, entry)?;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(entry);

    while let Some(block_id) = queue.pop_front() {
        if !visited.insert(block_id) {
            continue;
        }
        let block = get_block(blocks, block_id)?;
        for succ in block_successors(block) {
            if !visited.contains(&succ) {
                queue.push_back(succ);
            }
        }
    }

    // Only emit reachable blocks to keep the JS switch compact and avoid dead cases.
    Ok(visited.into_iter().collect())
}

fn block_successors(block: &HirBlock) -> Vec<BlockId> {
    let mut successors = Vec::new();
    for node in &block.nodes {
        if let HirKind::Terminator(term) = &node.kind {
            match term {
                HirTerminator::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    successors.push(*then_block);
                    if let Some(else_block) = else_block {
                        successors.push(*else_block);
                    }
                }
                HirTerminator::Match {
                    arms,
                    default_block,
                    ..
                } => {
                    for arm in arms {
                        successors.push(arm.body);
                    }
                    if let Some(default_block) = default_block {
                        successors.push(*default_block);
                    }
                }
                HirTerminator::Loop { body, label, .. } => {
                    successors.push(*body);
                    successors.push(*label);
                }
                HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                    successors.push(*target);
                }
                HirTerminator::Return(_)
                | HirTerminator::ReturnError(_)
                | HirTerminator::Panic { .. } => {}
            }
        }
    }
    successors
}

fn scan_expr(expr: &HirExpr, uses_template_results: &mut bool) {
    match &expr.kind {
        HirExprKind::HeapString(_) => {
            // Heap strings read from the template-results map at runtime.
            *uses_template_results = true;
        }
        HirExprKind::Load(place) | HirExprKind::Move(place) => {
            scan_place(place, uses_template_results);
        }
        HirExprKind::BinOp { left, right, .. } => {
            scan_expr(left, uses_template_results);
            scan_expr(right, uses_template_results);
        }
        HirExprKind::UnaryOp { operand, .. } => {
            scan_expr(operand, uses_template_results);
        }
        HirExprKind::Call { args, .. } => {
            for arg in args {
                scan_expr(arg, uses_template_results);
            }
        }
        HirExprKind::MethodCall { receiver, args, .. } => {
            scan_expr(receiver, uses_template_results);
            for arg in args {
                scan_expr(arg, uses_template_results);
            }
        }
        HirExprKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                scan_expr(value, uses_template_results);
            }
        }
        HirExprKind::Collection(items) => {
            for item in items {
                scan_expr(item, uses_template_results);
            }
        }
        HirExprKind::Range { start, end } => {
            scan_expr(start, uses_template_results);
            scan_expr(end, uses_template_results);
        }
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_)
        | HirExprKind::Field { .. } => {}
    }
}

fn scan_place(place: &HirPlace, uses_template_results: &mut bool) {
    match place {
        HirPlace::Var(_) => {}
        HirPlace::Field { base, .. } => scan_place(base, uses_template_results),
        HirPlace::Index { base, index } => {
            scan_place(base, uses_template_results);
            scan_expr(index, uses_template_results);
        }
    }
}

fn scan_terminator(
    terminator: &HirTerminator,
    locals: &mut HashSet<InternedString>,
    uses_template_results: &mut bool,
    params: &HashSet<InternedString>,
) {
    match terminator {
        HirTerminator::If { condition, .. } => {
            scan_expr(condition, uses_template_results);
        }
        HirTerminator::Match {
            scrutinee, arms, ..
        } => {
            scan_expr(scrutinee, uses_template_results);
            for arm in arms {
                match &arm.pattern {
                    crate::compiler::hir::nodes::HirPattern::Literal(expr) => {
                        scan_expr(expr, uses_template_results);
                    }
                    crate::compiler::hir::nodes::HirPattern::Range { start, end } => {
                        scan_expr(start, uses_template_results);
                        scan_expr(end, uses_template_results);
                    }
                    crate::compiler::hir::nodes::HirPattern::Wildcard => {}
                }
                if let Some(guard) = &arm.guard {
                    scan_expr(guard, uses_template_results);
                }
            }
        }
        HirTerminator::Loop {
            binding,
            iterator,
            index_binding,
            ..
        } => {
            if let Some((name, _)) = binding {
                if !params.contains(name) {
                    locals.insert(*name);
                }
            }
            if let Some(index_name) = index_binding {
                if !params.contains(index_name) {
                    locals.insert(*index_name);
                }
            }
            if let Some(iterator_expr) = iterator {
                scan_expr(iterator_expr, uses_template_results);
            }
        }
        HirTerminator::Return(values) => {
            for value in values {
                scan_expr(value, uses_template_results);
            }
        }
        HirTerminator::ReturnError(expr) => {
            scan_expr(expr, uses_template_results);
        }
        HirTerminator::Panic { message } => {
            if let Some(expr) = message {
                scan_expr(expr, uses_template_results);
            }
        }
        HirTerminator::Break { .. } | HirTerminator::Continue { .. } => {}
    }
}

fn get_block<'a>(blocks: &'a [HirBlock], id: BlockId) -> Result<&'a HirBlock, CompilerError> {
    blocks
        .get(id)
        .ok_or_else(|| CompilerError::compiler_error(format!("Invalid HIR block id {}", id)))
}
