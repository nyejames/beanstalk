//! Synthetic Beandown header preparation.
//!
//! WHAT: turns a tokenized `.bd` body into the normal private `content #String`
//! declaration consumed by dependency sorting and AST.
//! WHY: Beandown source is authored as a template body, but later frontend stages
//! should see an ordinary constant header instead of a Beandown-specific AST path
//! or textually wrapped source.

use crate::compiler_frontend::compiler_messages::source_location::CharPosition;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::binding_mode::BindingMode;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::headers::types::{
    FileFrontendPrepareOutput, FileRole, Header, HeaderExportMode, HeaderKind,
};
use crate::compiler_frontend::symbols::identity::FileId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::utilities::token_scan::collect_symbol_references;
use std::collections::HashSet;
use std::path::PathBuf;

const BEANDOWN_CONTENT_NAME: &str = "content";
const BEANDOWN_MARKDOWN_DIRECTIVE: &str = "markdown";

/// Build the header-stage output for one `.bd` source file.
///
/// The input token stream must already have been tokenized with Beandown's template-body entry
/// policy. This function adds only structural wrapper tokens around those body tokens; it never
/// prepends or appends source text.
pub(crate) fn prepare_beandown_file(
    file_tokens: FileTokens,
    string_table: &mut StringTable,
) -> FileFrontendPrepareOutput {
    let token_count = file_tokens.length;
    let context = BeandownPrepareContext::new(file_tokens, string_table);
    let content_header = context.content_header();

    FileFrontendPrepareOutput {
        source_file: context.source_file,
        file_id: context.file_id,
        token_count,
        file_role: FileRole::Normal,
        file_imports: Vec::new(),
        canonical_os_path: context.canonical_os_path,
        headers: vec![content_header],
        top_level_const_fragments: Vec::new(),
        const_template_count: 0,
        runtime_fragment_count: 0,
        warnings: Vec::new(),
    }
}

/// File-local data needed to synthesize the normal constant header.
///
/// Keeping these fields together makes the generated token construction explicit without
/// threading the same path, location, and interned names through every helper.
struct BeandownPrepareContext {
    source_file: InternedPath,
    file_id: Option<FileId>,
    canonical_os_path: Option<PathBuf>,
    body_tokens: Vec<Token>,
    synthetic_location: SourceLocation,
    content_name: StringId,
    markdown_directive: StringId,
}

impl BeandownPrepareContext {
    fn new(file_tokens: FileTokens, string_table: &mut StringTable) -> Self {
        let synthetic_location = SourceLocation::new(
            file_tokens.src_path.clone(),
            CharPosition::default(),
            CharPosition::default(),
        );
        let content_name = string_table.intern(BEANDOWN_CONTENT_NAME);
        let markdown_directive = string_table.intern(BEANDOWN_MARKDOWN_DIRECTIVE);

        let body_tokens = file_tokens
            .tokens
            .into_iter()
            .filter(|token| !matches!(token.kind, TokenKind::ModuleStart | TokenKind::Eof))
            .collect();

        Self {
            source_file: file_tokens.src_path,
            file_id: file_tokens.file_id,
            canonical_os_path: file_tokens.canonical_os_path,
            body_tokens,
            synthetic_location,
            content_name,
            markdown_directive,
        }
    }

    fn content_header(&self) -> Header {
        let declaration = self.content_declaration();
        let header_path = self.source_file.append(self.content_name);
        let mut header_tokens = FileTokens::new_with_file_id(header_path, self.file_id, Vec::new());
        header_tokens.canonical_os_path = self.canonical_os_path.clone();

        Header {
            kind: HeaderKind::Constant { declaration },
            file_role: FileRole::Normal,
            export_mode: HeaderExportMode::Private,
            dependencies: HashSet::new(),
            name_location: self.synthetic_location.clone(),
            tokens: header_tokens,
            source_file: self.source_file.clone(),
            capacity_references: Vec::new(),
        }
    }

    fn content_declaration(&self) -> DeclarationSyntax {
        let initializer_tokens = self.template_initializer_tokens();
        let initializer_references = collect_symbol_references(&initializer_tokens);

        DeclarationSyntax {
            binding_mode: BindingMode::CompileTimeConstant,
            type_annotation: ParsedTypeRef::BuiltinString {
                location: self.synthetic_location.clone(),
            },
            initializer_tokens,
            initializer_references,
            location: self.synthetic_location.clone(),
        }
    }

    fn template_initializer_tokens(&self) -> Vec<Token> {
        let mut initializer_tokens = Vec::with_capacity(self.body_tokens.len() + 4);

        initializer_tokens.push(self.synthetic_token(TokenKind::TemplateHead));
        initializer_tokens
            .push(self.synthetic_token(TokenKind::StyleDirective(self.markdown_directive)));
        initializer_tokens.push(self.synthetic_token(TokenKind::StartTemplateBody));
        initializer_tokens.extend(self.body_tokens.iter().cloned());
        initializer_tokens.push(self.synthetic_token(TokenKind::TemplateClose));

        initializer_tokens
    }

    fn synthetic_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, self.synthetic_location.clone())
    }
}

#[cfg(test)]
#[path = "tests/beandown_prepare_tests.rs"]
mod beandown_prepare_tests;
