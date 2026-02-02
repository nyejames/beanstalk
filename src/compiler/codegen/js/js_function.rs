use crate::compiler::hir::nodes::BlockId;
use crate::compiler::string_interning::InternedString;

/// What kind of callable is currently being lowered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    Function,
    Template,
}

/// Context for lowering a single function or template
pub struct CallableContext {
    pub kind: CallableKind,
    pub name: InternedString,
    pub body: BlockId,
}
