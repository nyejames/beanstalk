pub(crate) mod build_hir;
pub(crate) mod control_flow_linearizer;
mod display_hir;
pub(crate) mod errors;
pub(crate) mod expression_linearizer;
pub(crate) mod function_transformer;
pub(crate) mod nodes;
pub(crate) mod struct_handler;
pub(crate) mod template_processor;
mod validator;
pub(crate) mod variable_manager;

pub(crate) mod memory_management {
    pub(crate) mod drop_point_inserter;
}
