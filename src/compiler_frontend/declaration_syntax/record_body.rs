//! Shared record-body parsing for declaration shells.
//!
//! WHAT: parses `| field Type [= default], ... |` bodies used by structs and choice payloads.
//! WHY: record bodies are a neutral declaration syntax concept, not struct-specific logic.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    SignatureMemberContext, SignatureMemberSyntax, parse_signature_members_syntax,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;

/// Boxed diagnostic result for record-body parsing.
///
/// WHAT: keeps the `| ... |` record-body parse on a small error boundary while
///       preserving structured diagnostics for struct and choice callers.
/// WHY: record-body parsing otherwise carries the large diagnostic value
///      through every successful header parse. Each caller unboxes once.
type RecordBodyParseResult = Result<Vec<SignatureMemberSyntax>, Box<CompilerDiagnostic>>;

/// Parse a record body from `| field Type [= default], ... |` syntax.
pub fn parse_record_body(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
    warnings: &mut Vec<CompilerDiagnostic>,
    member_context: SignatureMemberContext,
    owner_path: &InternedPath,
) -> RecordBodyParseResult {
    token_stream.advance();

    let fields = parse_signature_members_syntax(
        token_stream,
        string_table,
        warnings,
        member_context,
        owner_path,
    )?;

    token_stream.advance();

    Ok(fields)
}
