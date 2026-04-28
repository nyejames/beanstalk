//! Shared record-body parsing for declaration shells.
//!
//! WHAT: parses `| field Type [= default], ... |` bodies used by structs and choice payloads.
//! WHY: record bodies are a neutral declaration syntax concept, not struct-specific logic.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    SignatureMemberContext, parse_signature_members,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;

/// Parse a record body from `| field Type [= default], ... |` syntax.
pub fn parse_record_body(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
    member_context: SignatureMemberContext,
    owner_path: &InternedPath,
) -> Result<Vec<Declaration>, CompilerError> {
    token_stream.advance();

    let fields = parse_signature_members(
        token_stream,
        string_table,
        context,
        member_context,
        owner_path,
    )?;

    token_stream.advance();

    Ok(fields)
}
