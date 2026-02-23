use crate::backends::js::JsEmitter;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirFunction, HirMatchArm, HirPattern, HirTerminator,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlFlowStrategy {
    Structured,
    Dispatcher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchTermination {
    Jump(BlockId),
    Terminated,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_function(&mut self, function: &HirFunction) -> Result<(), CompilerError> {
        let function_name = self.function_name(function.id)?.to_owned();

        let mut parameters = Vec::with_capacity(function.params.len());
        for parameter_local in &function.params {
            parameters.push(self.local_name(*parameter_local)?.to_owned());
        }

        self.emit_line(&format!(
            "function {}({}) {{",
            function_name,
            parameters.join(", ")
        ));

        self.indent += 1;

        let reachable_blocks = self.collect_reachable_blocks(function.entry)?;
        self.emit_function_local_declarations(function, &reachable_blocks)?;

        let strategy = self.choose_control_flow_strategy(function, &reachable_blocks)?;

        match strategy {
            ControlFlowStrategy::Structured => {
                self.emit_structured_function_body(function)?;
            }
            ControlFlowStrategy::Dispatcher => {
                self.emit_dispatcher_for_function(function, &reachable_blocks)?;
            }
        }

        self.indent -= 1;
        self.emit_line("}");

        Ok(())
    }

    fn emit_function_local_declarations(
        &mut self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
    ) -> Result<(), CompilerError> {
        let parameter_set = function.params.iter().copied().collect::<HashSet<_>>();
        let mut local_ids = Vec::new();

        for block_id in reachable_blocks {
            let block = self.block_by_id(*block_id)?;
            for local in &block.locals {
                if !parameter_set.contains(&local.id) {
                    local_ids.push(local.id);
                }
            }
        }

        local_ids.sort_by_key(|local_id| local_id.0);
        local_ids.dedup_by_key(|local_id| local_id.0);

        for local_id in local_ids {
            let local_name = self.local_name(local_id)?;
            self.emit_line(&format!("let {};", local_name));
        }

        if !reachable_blocks.is_empty() {
            self.emit_line("");
        }

        Ok(())
    }

    fn choose_control_flow_strategy(
        &self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
    ) -> Result<ControlFlowStrategy, CompilerError> {
        if self.has_cfg_cycle(function.entry)? {
            return Ok(ControlFlowStrategy::Dispatcher);
        }

        for block_id in reachable_blocks {
            let block = self.block_by_id(*block_id)?;

            match &block.terminator {
                HirTerminator::Jump { args, .. } => {
                    if !args.is_empty() {
                        return Ok(ControlFlowStrategy::Dispatcher);
                    }
                }

                HirTerminator::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    if self.inspect_simple_branch_termination(*then_block).is_err()
                        || self.inspect_simple_branch_termination(*else_block).is_err()
                    {
                        return Ok(ControlFlowStrategy::Dispatcher);
                    }
                }

                HirTerminator::Match { arms, .. } => {
                    if arms.is_empty() {
                        return Ok(ControlFlowStrategy::Dispatcher);
                    }

                    for arm in arms {
                        if !matches!(arm.pattern, HirPattern::Literal(_) | HirPattern::Wildcard) {
                            return Ok(ControlFlowStrategy::Dispatcher);
                        }

                        if self.inspect_simple_branch_termination(arm.body).is_err() {
                            return Ok(ControlFlowStrategy::Dispatcher);
                        }
                    }
                }

                HirTerminator::Loop { .. }
                | HirTerminator::Break { .. }
                | HirTerminator::Continue { .. } => {
                    return Ok(ControlFlowStrategy::Dispatcher);
                }

                HirTerminator::Return(_) | HirTerminator::Panic { .. } => {}
            }
        }

        Ok(ControlFlowStrategy::Structured)
    }

    fn has_cfg_cycle(&self, entry_block: BlockId) -> Result<bool, CompilerError> {
        fn dfs(
            emitter: &JsEmitter<'_>,
            block_id: BlockId,
            visiting: &mut HashSet<BlockId>,
            visited: &mut HashSet<BlockId>,
        ) -> Result<bool, CompilerError> {
            if visiting.contains(&block_id) {
                return Ok(true);
            }

            if visited.contains(&block_id) {
                return Ok(false);
            }

            visiting.insert(block_id);

            let block = emitter.block_by_id(block_id)?;
            for successor in JsEmitter::terminator_successors(&block.terminator) {
                if dfs(emitter, successor, visiting, visited)? {
                    return Ok(true);
                }
            }

            visiting.remove(&block_id);
            visited.insert(block_id);

            Ok(false)
        }

        dfs(self, entry_block, &mut HashSet::new(), &mut HashSet::new())
    }

    fn emit_structured_function_body(
        &mut self,
        function: &HirFunction,
    ) -> Result<(), CompilerError> {
        let mut emitted_blocks = HashSet::new();
        self.emit_structured_block(function.entry, &mut emitted_blocks)
    }

    fn emit_structured_block(
        &mut self,
        block_id: BlockId,
        emitted_blocks: &mut HashSet<BlockId>,
    ) -> Result<(), CompilerError> {
        if emitted_blocks.contains(&block_id) {
            return Ok(());
        }

        emitted_blocks.insert(block_id);

        let block = self.block_by_id(block_id)?.clone();
        self.emit_block_statements(&block)?;

        match &block.terminator {
            HirTerminator::Jump { target, args } => {
                if !args.is_empty() {
                    return Err(CompilerError::compiler_error(
                        "JavaScript backend: Jump terminator args are not supported yet",
                    ));
                }

                self.emit_structured_block(*target, emitted_blocks)
            }

            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => self.emit_structured_if(condition, *then_block, *else_block, emitted_blocks),

            HirTerminator::Match { scrutinee, arms } => {
                self.emit_structured_match(scrutinee, arms, emitted_blocks)
            }

            HirTerminator::Return(expression) => self.emit_return_terminator(expression),
            HirTerminator::Panic { message } => self.emit_panic_terminator(message),

            HirTerminator::Loop { .. }
            | HirTerminator::Break { .. }
            | HirTerminator::Continue { .. } => Err(CompilerError::compiler_error(
                "JavaScript backend: structured lowering does not support Loop/Break/Continue terminators",
            )),
        }
    }

    fn emit_structured_if(
        &mut self,
        condition: &crate::compiler_frontend::hir::hir_nodes::HirExpression,
        then_block: BlockId,
        else_block: BlockId,
        emitted_blocks: &mut HashSet<BlockId>,
    ) -> Result<(), CompilerError> {
        let then_termination = self.inspect_simple_branch_termination(then_block)?;
        let else_termination = self.inspect_simple_branch_termination(else_block)?;
        let merge_target = Self::resolve_branch_merge_target(then_termination, else_termination)?;

        let condition = self.lower_expr(condition)?;

        self.emit_line(&format!("if ({}) {{", condition));
        self.indent += 1;
        self.emit_simple_branch_block(then_block, merge_target, emitted_blocks)?;
        self.indent -= 1;

        self.emit_line("} else {");
        self.indent += 1;
        self.emit_simple_branch_block(else_block, merge_target, emitted_blocks)?;
        self.indent -= 1;
        self.emit_line("}");

        if let Some(merge_target) = merge_target {
            self.emit_structured_block(merge_target, emitted_blocks)?;
        }

        Ok(())
    }

    fn emit_structured_match(
        &mut self,
        scrutinee: &crate::compiler_frontend::hir::hir_nodes::HirExpression,
        arms: &[HirMatchArm],
        emitted_blocks: &mut HashSet<BlockId>,
    ) -> Result<(), CompilerError> {
        let merge_target = self.resolve_match_merge_target(arms)?;
        let scrutinee = self.lower_expr(scrutinee)?;
        let scrutinee_temp = self.next_temp_identifier("__match_value");

        self.emit_line(&format!("const {} = {};", scrutinee_temp, scrutinee));

        for (index, arm) in arms.iter().enumerate() {
            let condition = self.lower_match_arm_condition(&scrutinee_temp, arm)?;

            if index == 0 {
                self.emit_line(&format!("if ({}) {{", condition));
            } else {
                self.emit_line(&format!("else if ({}) {{", condition));
            }

            self.indent += 1;
            self.emit_simple_branch_block(arm.body, merge_target, emitted_blocks)?;
            self.indent -= 1;
            self.emit_line("}");
        }

        if let Some(merge_target) = merge_target {
            self.emit_structured_block(merge_target, emitted_blocks)?;
        }

        Ok(())
    }

    fn emit_simple_branch_block(
        &mut self,
        block_id: BlockId,
        expected_merge_target: Option<BlockId>,
        emitted_blocks: &mut HashSet<BlockId>,
    ) -> Result<BranchTermination, CompilerError> {
        if emitted_blocks.contains(&block_id) {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: branch block was emitted more than once during structured lowering",
            ));
        }

        emitted_blocks.insert(block_id);

        let block = self.block_by_id(block_id)?.clone();
        self.emit_block_statements(&block)?;

        match &block.terminator {
            HirTerminator::Jump { target, args } => {
                if !args.is_empty() {
                    return Err(CompilerError::compiler_error(
                        "JavaScript backend: Jump terminator args are not supported yet",
                    ));
                }

                if expected_merge_target == Some(*target) {
                    Ok(BranchTermination::Jump(*target))
                } else {
                    Err(CompilerError::compiler_error(
                        "JavaScript backend: structured branch jumped to unexpected target",
                    ))
                }
            }

            HirTerminator::Return(expression) => {
                self.emit_return_terminator(expression)?;
                Ok(BranchTermination::Terminated)
            }

            HirTerminator::Panic { message } => {
                self.emit_panic_terminator(message)?;
                Ok(BranchTermination::Terminated)
            }

            _ => Err(CompilerError::compiler_error(
                "JavaScript backend: structured lowering encountered unsupported branch terminator",
            )),
        }
    }

    fn inspect_simple_branch_termination(
        &self,
        block_id: BlockId,
    ) -> Result<BranchTermination, CompilerError> {
        let block = self.block_by_id(block_id)?;

        match &block.terminator {
            HirTerminator::Jump { target, args } => {
                if !args.is_empty() {
                    return Err(CompilerError::compiler_error(
                        "JavaScript backend: Jump terminator args are not supported yet",
                    ));
                }

                Ok(BranchTermination::Jump(*target))
            }

            HirTerminator::Return(_) | HirTerminator::Panic { .. } => {
                Ok(BranchTermination::Terminated)
            }

            _ => Err(CompilerError::compiler_error(
                "JavaScript backend: branch terminator is not simple enough for structured lowering",
            )),
        }
    }

    fn resolve_branch_merge_target(
        then_termination: BranchTermination,
        else_termination: BranchTermination,
    ) -> Result<Option<BlockId>, CompilerError> {
        match (then_termination, else_termination) {
            (BranchTermination::Jump(then_target), BranchTermination::Jump(else_target)) => {
                if then_target == else_target {
                    Ok(Some(then_target))
                } else {
                    Err(CompilerError::compiler_error(
                        "JavaScript backend: structured if-branches jump to different merge targets",
                    ))
                }
            }

            (BranchTermination::Jump(target), BranchTermination::Terminated)
            | (BranchTermination::Terminated, BranchTermination::Jump(target)) => Ok(Some(target)),

            (BranchTermination::Terminated, BranchTermination::Terminated) => Ok(None),
        }
    }

    fn resolve_match_merge_target(
        &self,
        arms: &[HirMatchArm],
    ) -> Result<Option<BlockId>, CompilerError> {
        let mut jump_targets = Vec::new();

        for arm in arms {
            if let BranchTermination::Jump(target) =
                self.inspect_simple_branch_termination(arm.body)?
            {
                jump_targets.push(target);
            }
        }

        jump_targets.sort_by_key(|target| target.0);
        jump_targets.dedup_by_key(|target| target.0);

        match jump_targets.as_slice() {
            [] => Ok(None),
            [single] => Ok(Some(*single)),
            _ => Err(CompilerError::compiler_error(
                "JavaScript backend: structured match arms jump to different merge targets",
            )),
        }
    }
}
