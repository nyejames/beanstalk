//! Neutral top-level declaration shell parsers shared between the header stage and AST.
//!
//! Top level declarations can be fully parsed by the header stage if they are structs or choices ,
//! So the AST stage and the header stage need to share the basic syntaxes of all these declarations.
//!
//! AST stage and headers also want to parse function signatures and type the same way,
//! so that logic is centralized in this module.

pub(crate) mod choice;
pub(crate) mod declaration_shell;
pub(crate) mod generic_parameters;
pub(crate) mod record_body;
pub(crate) mod signature_members;
pub(crate) mod r#struct;
pub(crate) mod type_syntax;
