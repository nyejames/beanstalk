//! Token definitions and source-location primitives for the frontend tokenizer.
//!
//! WHAT: defines token kinds, token records, and the location metadata threaded through parsing.
//! WHY: every frontend stage past lexing depends on one canonical token and location model.

use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::identity::FileId;
use crate::compiler_frontend::interned_path::InternedPath;
pub use crate::compiler_frontend::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::token_log;
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenizeMode {
    Normal,
    TemplateBody,
    TemplateHead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TemplateBodyMode {
    #[default]
    Normal,
    Balanced,
    DiscardBalanced,
}

impl TemplateBodyMode {
    pub fn is_balanced_mode(self) -> bool {
        matches!(
            self,
            TemplateBodyMode::Balanced | TemplateBodyMode::DiscardBalanced
        )
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub location: SourceLocation,
}

impl Token {
    pub fn new(kind: TokenKind, location: SourceLocation) -> Self {
        Self { kind, location }
    }
}

#[derive(Clone, Debug)]
pub struct FileTokens {
    pub tokens: Vec<Token>,
    pub src_path: InternedPath,
    /// Stable source-file identity for this token stream.
    ///
    /// WHAT: carries frontend file identity into downstream parsing stages.
    /// WHY: entry-file detection and diagnostics should not rely on comparing path text.
    pub file_id: Option<FileId>,
    /// Canonical filesystem source path for IO/path-resolution-only logic.
    pub canonical_os_path: Option<PathBuf>,
    pub index: usize,
    pub length: usize,
}

impl FileTokens {
    pub fn new(src_path: InternedPath, tokens: Vec<Token>) -> FileTokens {
        Self::new_with_identity(src_path, None, None, tokens)
    }

    pub fn new_with_file_id(
        src_path: InternedPath,
        file_id: Option<FileId>,
        tokens: Vec<Token>,
    ) -> FileTokens {
        Self::new_with_identity(src_path, file_id, None, tokens)
    }

    pub fn new_with_identity(
        src_path: InternedPath,
        file_id: Option<FileId>,
        canonical_os_path: Option<PathBuf>,
        tokens: Vec<Token>,
    ) -> FileTokens {
        FileTokens {
            length: tokens.len(),
            src_path,
            file_id,
            canonical_os_path,
            tokens,
            index: 0,
        }
    }

    pub fn current_token_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
    }

    pub fn current_token(&self) -> Token {
        self.tokens[self.index].clone()
    }

    /// This should never be called from a context where there is no previous token
    pub fn previous_token(&self) -> &TokenKind {
        &self.tokens[self.index - 1].kind
    }

    pub fn peek_next_token(&self) -> Option<&TokenKind> {
        self.tokens.get(self.index + 1).map(|token| &token.kind)
    }

    pub fn current_location(&self) -> SourceLocation {
        self.tokens[self.index].location.clone()
    }

    pub fn advance(&mut self) {
        if self.index >= self.tokens.len() {
            token_log!(Red "Compiler tried to advance past token stream bounds");
            return;
        }

        match &self.current_token_kind() {
            // Can't advance past End of File
            &TokenKind::Eof => {
                // Show a warning for compiler_frontend development purposes
                token_log!(Red "Compiler tried to advance past EOF");
            }

            _ => {
                self.index += 1;
            }
        }
    }

    pub fn skip_newlines(&mut self) {
        while self.index + 1 < self.length
            && matches!(self.current_token_kind(), TokenKind::Newline)
        {
            self.index += 1;
        }
    }
}

pub struct TokenStream<'a> {
    pub file_path: &'a InternedPath,
    pub chars: Peekable<Chars<'a>>,
    pub position: CharPosition,
    pub start_position: CharPosition,
    pub mode: TokenizeMode,
    // WHAT: Stack of per-template parsing frames.
    //
    // WHY: `]` must restore the exact parent mode for nested templates opened by
    // `[`, and template-body behaviour must stay local to the template that
    // declared its head directives.
    //
    // A single global mode (for example, `TokenizeMode::Codeblock`) is not enough:
    // nested template heads can appear while parsing another template head/body,
    // and parent/child templates can have different style directives. We therefore
    // keep code-specific state on the current template frame and pop it naturally
    // when that template closes.
    pub template_mode_stack: Vec<TemplateModeFrame>,
    pub newline_mode: NewlineMode,
}

// WHAT: Metadata for one template nesting level in the tokenizer.
//
// WHY: directives are declared in a template head, but affect only that template's
// body tokenization. This frame carries that intent across `:` (head -> body),
// tracks bracket balance for balanced body modes, and ensures nested templates
// cannot accidentally inherit or overwrite the parent's body behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateModeFrame {
    pub mode: TokenizeMode,
    pub body_mode: TemplateBodyMode,
    pub body_open_square_brackets: usize,
    pub body_closed_square_brackets: usize,
}

impl TemplateModeFrame {
    fn new(mode: TokenizeMode) -> Self {
        Self {
            mode,
            body_mode: TemplateBodyMode::Normal,
            body_open_square_brackets: 0,
            body_closed_square_brackets: 0,
        }
    }
}

impl<'a> TokenStream<'a> {
    pub fn new(
        source_code: &'a str,
        file_path: &'a InternedPath,
        mode: TokenizeMode,
        newline_mode: NewlineMode,
    ) -> Self {
        Self {
            file_path,
            chars: source_code.chars().peekable(),
            position: CharPosition::default(),
            start_position: Default::default(),
            mode,
            template_mode_stack: vec![TemplateModeFrame::new(mode)],
            newline_mode,
        }
    }

    pub fn next(&mut self) -> Option<char> {
        match self.chars.peek() {
            Some(c) => {
                if *c == '\n' {
                    self.position.line_number += 1;
                    self.position.char_column = 0;
                } else {
                    self.position.char_column += 1;
                }

                self.chars.next()
            }

            None => None,
        }
    }

    pub fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    pub fn new_location(&mut self) -> SourceLocation {
        let start_pos = self.start_position;
        self.update_start_position();
        SourceLocation::new(self.file_path.to_owned(), start_pos, self.position)
    }

    pub fn update_start_position(&mut self) {
        self.start_position = self.position;
    }

    pub fn push_template_mode(&mut self, mode: TokenizeMode) {
        self.template_mode_stack.push(TemplateModeFrame::new(mode));
        self.mode = mode;
    }

    pub fn set_current_template_mode(&mut self, mode: TokenizeMode) {
        // `:` switches the current template from head parsing to body parsing
        // without closing the template nesting level, so mutate the top frame.
        if let Some(current_mode) = self.template_mode_stack.last_mut() {
            current_mode.mode = mode;
            if mode == TokenizeMode::TemplateBody && current_mode.body_mode.is_balanced_mode() {
                // Balanced template-body modes terminate only when square brackets are
                // balanced. The opening `[` that started this template counts as one open.
                current_mode.body_open_square_brackets = 1;
                current_mode.body_closed_square_brackets = 0;
            }
        } else {
            self.template_mode_stack.push(TemplateModeFrame::new(mode));
        }

        self.mode = mode;
    }

    pub fn pop_template_mode(&mut self) {
        // `]` closes exactly one template nesting level. Keep the initial frame so
        // tokenization started in a template mode cannot escape back to normal mode.
        if self.template_mode_stack.len() > 1 {
            self.template_mode_stack.pop();
        }

        self.mode = *self
            .template_mode_stack
            .last()
            .map(|frame| &frame.mode)
            .unwrap_or(&TokenizeMode::Normal);
    }

    pub fn mark_current_template_body_mode(&mut self, body_mode: TemplateBodyMode) {
        if let Some(current_mode) = self.template_mode_stack.last_mut() {
            current_mode.body_mode = body_mode;
            if current_mode.mode == TokenizeMode::TemplateBody && body_mode.is_balanced_mode() {
                current_mode.body_open_square_brackets = 1;
                current_mode.body_closed_square_brackets = 0;
            }
        }
    }

    pub fn current_template_body_mode(&self) -> TemplateBodyMode {
        self.template_mode_stack
            .last()
            .map(|frame| frame.body_mode)
            .unwrap_or_default()
    }

    pub fn register_template_body_open_square_bracket(&mut self) {
        if let Some(current_mode) = self.template_mode_stack.last_mut()
            && current_mode.body_mode.is_balanced_mode()
        {
            current_mode.body_open_square_brackets =
                current_mode.body_open_square_brackets.saturating_add(1);
        }
    }

    pub fn register_template_body_close_square_bracket(&mut self) {
        if let Some(current_mode) = self.template_mode_stack.last_mut()
            && current_mode.body_mode.is_balanced_mode()
        {
            current_mode.body_closed_square_brackets =
                current_mode.body_closed_square_brackets.saturating_add(1);
        }
    }

    pub fn template_body_next_close_balances_brackets(&self) -> bool {
        let Some(current_mode) = self.template_mode_stack.last() else {
            return false;
        };

        if current_mode.mode != TokenizeMode::TemplateBody
            || !current_mode.body_mode.is_balanced_mode()
        {
            return false;
        }

        current_mode.body_closed_square_brackets.saturating_add(1)
            == current_mode.body_open_square_brackets
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum TokenKind {
    // For Compiler
    ModuleStart, // Contains module name space
    Eof,         // End of the file

    // Module Import
    /// For Wasm files or host environment - importing from a different module or the host
    Import,

    // #
    Hash,

    /// Function Signatures
    Arrow,

    /// Variable name
    Symbol(StringId),
    // `$markdown`, `$fresh`, and builder-registered directives inside template heads.
    StyleDirective(StringId),

    // Values
    StringSliceLiteral(StringId),
    Path(Vec<InternedPath>), // Compile time path resolution
    FloatLiteral(f64),
    IntLiteral(i64),
    CharLiteral(char),
    RawStringLiteral(StringId),
    BoolLiteral(bool),

    // Collections
    OpenCurly,  // {
    CloseCurly, // }

    TypeParameterBracket, // |

    // Structure of Syntax
    Newline,
    End,
    StartTemplateBody,

    // Basic Grammar
    Comma,
    Dot,
    Colon,       // :
    DoubleColon, // ::
    Assign,      // =

    // Reserved trait syntax
    Must,
    TraitThis,

    // Scope
    OpenParenthesis,  // (
    CloseParenthesis, // )

    As,

    // Can modify types to become variadic parameters.
    // So any number of values can be passed in
    Variadic, // ..

    // Type Declarations
    Mutable,

    // Datatypes
    DatatypeNone,
    NoneLiteral,
    DatatypeInt,
    DatatypeFloat,
    DatatypeBool,
    DatatypeTrue,
    DatatypeFalse,
    DatatypeString,
    DatatypeChar,

    /// For Errors
    Bang,
    /// For Options
    QuestionMark,

    // Mathematical Operators
    Negative,

    Exponent,
    Multiply,
    Divide,
    Modulus,
    Remainder,
    Root,

    ExponentAssign,
    MultiplyAssign,
    DivideAssign,
    ModulusAssign,
    RootAssign,
    RemainderAssign,

    Add,
    Subtract,
    AddAssign,
    SubtractAssign,

    // Logical Operators in order of precedence
    Not,
    Is,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,

    And,
    Or,

    // Control Flow
    /// If statements and match statements
    If,
    Else,
    Return,

    // Loops
    Loop,
    In,
    By,
    Break,
    Continue,
    ExclusiveRange, // to
    InclusiveRange, // upto

    // Pattern matching
    Case,     // case
    FatArrow, // =>
    Wildcard, // _

    // Memory Management
    Copy,

    // Templates
    TemplateClose,
    TemplateHead,

    // Channels
    ChannelSend,    // >>
    ChannelReceive, // <<
    Yield,
}

impl TokenKind {
    pub fn to_datatype(&self) -> Option<DataType> {
        match self {
            TokenKind::DatatypeInt => Some(DataType::Int),
            TokenKind::DatatypeFloat => Some(DataType::Float),
            TokenKind::DatatypeBool => Some(DataType::Bool),
            TokenKind::DatatypeString => Some(DataType::StringSlice),
            TokenKind::DatatypeChar => Some(DataType::Char),
            _ => None,
        }
    }

    // For figuring out when to break out of or continue expressions and statements
    pub fn continues_expression(&self) -> bool {
        matches!(
            self,
            // Tokens that allow any number of newlines after or before them without breaking a statement or expression,
            TokenKind::Colon
                | TokenKind::OpenParenthesis
                | TokenKind::TypeParameterBracket
                | TokenKind::Comma
                | TokenKind::End
                | TokenKind::Assign
                | TokenKind::AddAssign
                | TokenKind::SubtractAssign
                | TokenKind::MultiplyAssign
                | TokenKind::DivideAssign
                | TokenKind::ExponentAssign
                | TokenKind::RootAssign
                | TokenKind::Add
                | TokenKind::Subtract
                | TokenKind::Multiply
                | TokenKind::Divide
                | TokenKind::Modulus
                | TokenKind::Root
                | TokenKind::Arrow
                | TokenKind::Is
                | TokenKind::LessThan
                | TokenKind::LessThanOrEqual
                | TokenKind::GreaterThan
                | TokenKind::GreaterThanOrEqual
        )
    }
}
