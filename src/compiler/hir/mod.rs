pub(crate) mod build_hir;
mod control_flow_linearizer;
mod display_hir;
mod errors;
mod expression_linearizer;
mod function_transformer;
pub(crate) mod nodes;
mod struct_handler;
mod template_processor;
mod validator;
mod variable_manager;

mod memory_management {
    pub(crate) mod drop_point_inserter;
}
