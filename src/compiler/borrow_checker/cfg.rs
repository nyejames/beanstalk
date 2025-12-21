//! Control Flow Graph construction from HIR nodes.
//!
//! Builds CFG for path-sensitive borrow checking analysis and lifetime inference.

use crate::compiler::borrow_checker::types::{CfgNodeType, ControlFlowGraph};
use crate::compiler::hir::nodes::{HirKind, HirNode};

/// Construct a control flow graph from HIR nodes.
///
/// Builds CFG by analyzing HIR node structure and creating appropriate
/// edges for all possible execution paths.
pub fn construct_cfg(hir_nodes: &[HirNode]) -> ControlFlowGraph {
    let mut cfg = ControlFlowGraph::new();

    if hir_nodes.is_empty() {
        return cfg;
    }

    // Add all nodes to the CFG first (including nested nodes)
    add_all_nodes_to_cfg(&mut cfg, hir_nodes);

    // Build edges between nodes
    build_cfg_edges(&mut cfg, hir_nodes);

    // Identify entry and exit points
    identify_entry_exit_points(&mut cfg, hir_nodes);

    cfg
}

/// Recursively add all HIR nodes to the CFG, including nested nodes
fn add_all_nodes_to_cfg(cfg: &mut ControlFlowGraph, hir_nodes: &[HirNode]) {
    for node in hir_nodes {
        let node_type = classify_node(node);
        cfg.add_node(node.id, node_type);

        // Recursively add nested nodes
        match &node.kind {
            HirKind::If {
                then_block,
                else_block,
                ..
            } => {
                add_all_nodes_to_cfg(cfg, then_block);
                if let Some(else_nodes) = else_block {
                    add_all_nodes_to_cfg(cfg, else_nodes);
                }
            }

            HirKind::Match { arms, default, .. } => {
                for arm in arms {
                    add_all_nodes_to_cfg(cfg, &arm.body);
                }
                if let Some(default_nodes) = default {
                    add_all_nodes_to_cfg(cfg, default_nodes);
                }
            }

            HirKind::Loop { body, .. } => {
                add_all_nodes_to_cfg(cfg, body);
            }

            HirKind::TryCall {
                call,
                error_handler,
                ..
            } => {
                // Add the call node
                let call_node_type = classify_node(call);
                cfg.add_node(call.id, call_node_type);

                // Add nested nodes in call if any
                if let HirKind::If {
                    then_block,
                    else_block,
                    ..
                } = &call.kind
                {
                    add_all_nodes_to_cfg(cfg, then_block);
                    if let Some(else_nodes) = else_block {
                        add_all_nodes_to_cfg(cfg, else_nodes);
                    }
                }

                // Add error handler nodes
                add_all_nodes_to_cfg(cfg, error_handler);
            }

            HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
                add_all_nodes_to_cfg(cfg, body);
            }

            _ => {} // No nested nodes for other types
        }
    }
}

/// Classify a HIR node for CFG construction
fn classify_node(node: &HirNode) -> CfgNodeType {
    match &node.kind {
        // Branch points
        HirKind::If { .. } | HirKind::Match { .. } | HirKind::TryCall { .. } => CfgNodeType::Branch,

        // Loop constructs
        HirKind::Loop { .. } => CfgNodeType::LoopHeader,

        // Function definitions
        HirKind::FunctionDef { .. } | HirKind::TemplateFn { .. } => CfgNodeType::FunctionEntry,

        // Function exits
        HirKind::Return(_) | HirKind::ReturnError(_) => CfgNodeType::FunctionExit,

        // Loop control flow
        HirKind::Break | HirKind::Continue => CfgNodeType::Statement, // Could be specialized later

        // All other nodes are statements
        _ => CfgNodeType::Statement,
    }
}

/// Build edges between CFG nodes based on control flow
fn build_cfg_edges(cfg: &mut ControlFlowGraph, hir_nodes: &[HirNode]) {
    for (i, node) in hir_nodes.iter().enumerate() {
        match &node.kind {
            // Sequential flow for most statements
            HirKind::Assign { .. }
            | HirKind::Borrow { .. }
            | HirKind::Call { .. }
            | HirKind::HostCall { .. }
            | HirKind::ExprStmt(_)
            | HirKind::Drop(_)
            | HirKind::StructDef { .. } => {
                // Connect to next node if it exists
                if i + 1 < hir_nodes.len() {
                    cfg.add_edge(node.id, hir_nodes[i + 1].id);
                }
            }

            // Conditional branches
            HirKind::If {
                then_block,
                else_block,
                ..
            } => {
                // Connect to then block
                if let Some(first_then) = then_block.first() {
                    cfg.add_edge(node.id, first_then.id);
                }

                // Connect to else block if it exists
                if let Some(else_nodes) = else_block {
                    if let Some(first_else) = else_nodes.first() {
                        cfg.add_edge(node.id, first_else.id);
                    }
                } else {
                    // If no else block, if condition can fall through to next node
                    if i + 1 < hir_nodes.len() {
                        cfg.add_edge(node.id, hir_nodes[i + 1].id);
                    }
                }

                // Build edges within blocks
                build_block_edges(cfg, then_block);
                if let Some(else_nodes) = else_block {
                    build_block_edges(cfg, else_nodes);
                }

                // Connect blocks to next node after if statement
                let next_node_id = if i + 1 < hir_nodes.len() {
                    Some(hir_nodes[i + 1].id)
                } else {
                    None
                };

                if let Some(next_id) = next_node_id {
                    // Connect end of then block
                    if let Some(last_then) = then_block.last() {
                        if !is_terminating_node(last_then) {
                            cfg.add_edge(last_then.id, next_id);
                        }
                    }

                    // Connect end of else block
                    if let Some(else_nodes) = else_block {
                        if let Some(last_else) = else_nodes.last() {
                            if !is_terminating_node(last_else) {
                                cfg.add_edge(last_else.id, next_id);
                            }
                        }
                    }
                }
            }

            // Match statements
            HirKind::Match { arms, default, .. } => {
                // Connect to each match arm
                for arm in arms {
                    if let Some(first_arm) = arm.body.first() {
                        cfg.add_edge(node.id, first_arm.id);
                    }
                    build_block_edges(cfg, &arm.body);
                }

                // Connect to default arm if it exists
                if let Some(default_nodes) = default {
                    if let Some(first_default) = default_nodes.first() {
                        cfg.add_edge(node.id, first_default.id);
                    }
                    build_block_edges(cfg, default_nodes);
                }

                // Connect arms to next node after match
                let next_node_id = if i + 1 < hir_nodes.len() {
                    Some(hir_nodes[i + 1].id)
                } else {
                    None
                };

                if let Some(next_id) = next_node_id {
                    // Connect end of each arm
                    for arm in arms {
                        if let Some(last_arm) = arm.body.last() {
                            if !is_terminating_node(last_arm) {
                                cfg.add_edge(last_arm.id, next_id);
                            }
                        }
                    }

                    // Connect end of default arm
                    if let Some(default_nodes) = default {
                        if let Some(last_default) = default_nodes.last() {
                            if !is_terminating_node(last_default) {
                                cfg.add_edge(last_default.id, next_id);
                            }
                        }
                    }
                }
            }

            // Loop statements
            HirKind::Loop { body, .. } => {
                // Connect loop header to body
                if let Some(first_body) = body.first() {
                    cfg.add_edge(node.id, first_body.id);
                }

                // Build edges within loop body
                build_block_edges(cfg, body);

                // Connect end of body back to loop header (for iteration)
                if let Some(last_body) = body.last() {
                    if !is_terminating_node(last_body) {
                        cfg.add_edge(last_body.id, node.id);
                    }
                }

                // Connect loop to next node (for loop exit)
                if i + 1 < hir_nodes.len() {
                    cfg.add_edge(node.id, hir_nodes[i + 1].id);
                }
            }

            // Try call with error handling
            HirKind::TryCall {
                call,
                error_handler,
                ..
            } => {
                // Connect to the call
                cfg.add_edge(node.id, call.id);

                // Build edges within the call (if it's a complex node)
                if let HirKind::Call { .. } | HirKind::HostCall { .. } = call.kind {
                    // Simple call - connect to error handler or next node
                    if !error_handler.is_empty() {
                        if let Some(first_handler) = error_handler.first() {
                            cfg.add_edge(call.id, first_handler.id);
                        }
                        build_block_edges(cfg, error_handler);
                    }
                }

                // Connect to next node after try call
                if i + 1 < hir_nodes.len() {
                    let next_id = hir_nodes[i + 1].id;

                    // Success path from call
                    cfg.add_edge(call.id, next_id);

                    // Error path from handler
                    if let Some(last_handler) = error_handler.last() {
                        if !is_terminating_node(last_handler) {
                            cfg.add_edge(last_handler.id, next_id);
                        }
                    }
                }
            }

            // Option unwrap
            HirKind::OptionUnwrap { .. } => {
                // Simple sequential flow for option unwrap
                if i + 1 < hir_nodes.len() {
                    cfg.add_edge(node.id, hir_nodes[i + 1].id);
                }
            }

            // Runtime template call
            HirKind::RuntimeTemplateCall { .. } => {
                // Sequential flow for template calls
                if i + 1 < hir_nodes.len() {
                    cfg.add_edge(node.id, hir_nodes[i + 1].id);
                }
            }

            // Template function definition
            HirKind::TemplateFn { body, .. } => {
                // Connect function entry to body
                if let Some(first_body) = body.first() {
                    cfg.add_edge(node.id, first_body.id);
                }

                // Build edges within function body
                build_block_edges(cfg, body);
            }

            // Function definitions
            HirKind::FunctionDef { body, .. } => {
                // Connect function entry to body
                if let Some(first_body) = body.first() {
                    cfg.add_edge(node.id, first_body.id);
                }

                // Build edges within function body
                build_block_edges(cfg, body);
            }

            // Terminating statements don't connect to next node
            HirKind::Return(_) | HirKind::ReturnError(_) => {
                // No outgoing edges for returns
            }

            // Break and continue statements
            HirKind::Break | HirKind::Continue => {
                // These are handled specially in loop context
                // For now, treat as terminating
            }
        }
    }
}

/// Build edges within a block of HIR nodes
fn build_block_edges(cfg: &mut ControlFlowGraph, block: &[HirNode]) {
    for (i, node) in block.iter().enumerate() {
        match &node.kind {
            // Handle structured control flow within blocks
            HirKind::If {
                then_block,
                else_block,
                ..
            } => {
                // Connect to then block
                if let Some(first_then) = then_block.first() {
                    cfg.add_edge(node.id, first_then.id);
                }

                // Connect to else block if it exists
                if let Some(else_nodes) = else_block {
                    if let Some(first_else) = else_nodes.first() {
                        cfg.add_edge(node.id, first_else.id);
                    }
                } else {
                    // If no else block, if condition can fall through to next node
                    if i + 1 < block.len() {
                        cfg.add_edge(node.id, block[i + 1].id);
                    }
                }

                // Recursively build edges within nested blocks
                build_block_edges(cfg, then_block);
                if let Some(else_nodes) = else_block {
                    build_block_edges(cfg, else_nodes);
                }

                // Connect blocks to next node after if statement
                if i + 1 < block.len() {
                    let next_id = block[i + 1].id;

                    // Connect end of then block
                    if let Some(last_then) = then_block.last() {
                        if !is_terminating_node(last_then) {
                            cfg.add_edge(last_then.id, next_id);
                        }
                    }

                    // Connect end of else block
                    if let Some(else_nodes) = else_block {
                        if let Some(last_else) = else_nodes.last() {
                            if !is_terminating_node(last_else) {
                                cfg.add_edge(last_else.id, next_id);
                            }
                        }
                    }
                }
            }

            HirKind::Match { arms, default, .. } => {
                // Connect to each match arm
                for arm in arms {
                    if let Some(first_arm) = arm.body.first() {
                        cfg.add_edge(node.id, first_arm.id);
                    }
                    build_block_edges(cfg, &arm.body);
                }

                // Connect to default arm if it exists
                if let Some(default_nodes) = default {
                    if let Some(first_default) = default_nodes.first() {
                        cfg.add_edge(node.id, first_default.id);
                    }
                    build_block_edges(cfg, default_nodes);
                }

                // Connect arms to next node after match
                if i + 1 < block.len() {
                    let next_id = block[i + 1].id;

                    // Connect end of each arm
                    for arm in arms {
                        if let Some(last_arm) = arm.body.last() {
                            if !is_terminating_node(last_arm) {
                                cfg.add_edge(last_arm.id, next_id);
                            }
                        }
                    }

                    // Connect end of default arm
                    if let Some(default_nodes) = default {
                        if let Some(last_default) = default_nodes.last() {
                            if !is_terminating_node(last_default) {
                                cfg.add_edge(last_default.id, next_id);
                            }
                        }
                    }
                }
            }

            HirKind::Loop { body, .. } => {
                // Connect loop header to body
                if let Some(first_body) = body.first() {
                    cfg.add_edge(node.id, first_body.id);
                }

                // Build edges within loop body
                build_block_edges(cfg, body);

                // Connect end of body back to loop header (for iteration)
                if let Some(last_body) = body.last() {
                    if !is_terminating_node(last_body) {
                        cfg.add_edge(last_body.id, node.id);
                    }
                }

                // Connect loop to next node (for loop exit)
                if i + 1 < block.len() {
                    cfg.add_edge(node.id, block[i + 1].id);
                }
            }

            HirKind::TryCall {
                call,
                error_handler,
                ..
            } => {
                // Connect to the call
                cfg.add_edge(node.id, call.id);

                // Connect to error handler
                if !error_handler.is_empty() {
                    if let Some(first_handler) = error_handler.first() {
                        cfg.add_edge(call.id, first_handler.id);
                    }
                    build_block_edges(cfg, error_handler);
                }

                // Connect to next node after try call
                if i + 1 < block.len() {
                    let next_id = block[i + 1].id;

                    // Success path from call
                    cfg.add_edge(call.id, next_id);

                    // Error path from handler
                    if let Some(last_handler) = error_handler.last() {
                        if !is_terminating_node(last_handler) {
                            cfg.add_edge(last_handler.id, next_id);
                        }
                    }
                }
            }

            // For all other nodes, simple sequential flow
            _ => {
                if i + 1 < block.len() && !is_terminating_node(node) {
                    cfg.add_edge(node.id, block[i + 1].id);
                }
            }
        }
    }
}

/// Check if a node terminates control flow (doesn't fall through)
fn is_terminating_node(node: &HirNode) -> bool {
    matches!(
        node.kind,
        HirKind::Return(_) | HirKind::ReturnError(_) | HirKind::Break | HirKind::Continue
    )
}

/// Identify entry and exit points in the CFG
fn identify_entry_exit_points(cfg: &mut ControlFlowGraph, hir_nodes: &[HirNode]) {
    for node in hir_nodes {
        match &node.kind {
            // Function definitions are entry points
            HirKind::FunctionDef { .. } | HirKind::TemplateFn { .. } => {
                cfg.add_entry_point(node.id);
            }

            // Returns are exit points
            HirKind::Return(_) | HirKind::ReturnError(_) => {
                cfg.add_exit_point(node.id);
            }

            _ => {}
        }
    }

    // If no explicit entry points, first node is entry
    if cfg.entry_points.is_empty() && !hir_nodes.is_empty() {
        cfg.add_entry_point(hir_nodes[0].id);
    }

    // If no explicit exit points, last non-terminating node is exit
    if cfg.exit_points.is_empty() {
        if let Some(last_node) = hir_nodes.last() {
            if !is_terminating_node(last_node) {
                cfg.add_exit_point(last_node.id);
            }
        }
    }
}
