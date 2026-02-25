use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};

use crate::compiler_frontend::compiler_errors::ErrorLocation;
use crate::token_log;
use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq)]
pub enum TokenizeMode {
    Normal,
    TemplateBody,
    TemplateHead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CharPosition {
    pub line_number: i32,
    pub char_column: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextLocation {
    pub scope: InternedPath,
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

impl TextLocation {
    pub fn new(scope: InternedPath, start: CharPosition, end: CharPosition) -> Self {
        Self {
            scope,
            start_pos: start,
            end_pos: end,
        }
    }

    pub fn new_just_line(start: i32) -> Self {
        Self {
            scope: InternedPath::new(),
            start_pos: CharPosition {
                line_number: start,
                char_column: 0,
            },
            end_pos: CharPosition {
                line_number: start,
                char_column: 120, // Arbitrary number
            },
        }
    }

    pub fn to_error_location(&self, string_table: &StringTable) -> ErrorLocation {
        ErrorLocation {
            scope: self.scope.to_path_buf(string_table),
            start_pos: self.start_pos,
            end_pos: self.end_pos,
        }
    }

    /// Converts to ErrorLocation without resolving the scope path.
    /// Uses an empty path - useful for HIR validation errors where
    /// the string table may not be available.
    pub fn to_error_location_without_table(&self) -> ErrorLocation {
        ErrorLocation {
            scope: std::path::PathBuf::new(),
            start_pos: self.start_pos,
            end_pos: self.end_pos,
        }
    }
}

impl PartialOrd for TextLocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Check if self's start position is before other's start position
        let self_start_line = self.start_pos.line_number;
        let other_start_line = other.start_pos.line_number;

        if self_start_line < other_start_line {
            // Self starts on an earlier line than other
            let self_end_line = self.end_pos.line_number;

            // If self ends before other starts, it's definitely less
            if self_end_line < other_start_line {
                Some(Ordering::Less)
            } else {
                // Self starts before but extends into or beyond other's range - considered equivalent
                Some(Ordering::Equal)
            }
        } else if self_start_line > other_start_line {
            // Self starts on a later line than other
            let other_end_line = other.end_pos.line_number;

            // If other ends before self starts, self is definitely greater
            if other_end_line < self_start_line {
                Some(Ordering::Greater)
            } else {
                // Other starts before but extends into or beyond self's range - considered equivalent
                Some(Ordering::Equal)
            }
        } else {
            // Same start line, compare columns
            let self_start_col = self.start_pos.char_column;
            let other_start_col = other.start_pos.char_column;

            if self_start_col < other_start_col {
                // Self starts before other on the same line
                let self_end_line = self.end_pos.line_number;
                let self_end_col = self.end_pos.char_column;

                // If self ends before other starts on the same line
                if self_end_line < other_start_line
                    || (self_end_line == other_start_line && self_end_col < other_start_col)
                {
                    Some(Ordering::Less)
                } else {
                    // Self overlaps with other - considered equivalent
                    Some(Ordering::Equal)
                }
            } else if self_start_col > other_start_col {
                // Other starts before self on the same line
                let other_end_line = other.end_pos.line_number;
                let other_end_col = other.end_pos.char_column;

                // If other ends before self starts on the same line
                if other_end_line < self_start_line
                    || (other_end_line == self_start_line && other_end_col < self_start_col)
                {
                    Some(Ordering::Greater)
                } else {
                    // Other overlaps with self - considered equivalent
                    Some(Ordering::Equal)
                }
            } else {
                // Exactly the same start position - considered equivalent
                Some(Ordering::Equal)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub location: TextLocation,
}

impl Token {
    pub fn new(kind: TokenKind, location: TextLocation) -> Self {
        Self { kind, location }
    }

    pub fn to_string(&self) -> String {
        format!("{:?}", self.kind)
    }

    /// Get the string content of this token if it contains string data.
    /// Returns the resolved string content for Symbol, StringSliceLiteral, RawStringLiteral, and PathLiteral tokens.
    /// Returns empty string for other token types.
    pub fn as_string(&self, string_table: &StringTable) -> String {
        match &self.kind {
            TokenKind::Symbol(id)
            | TokenKind::StringSliceLiteral(id)
            | TokenKind::RawStringLiteral(id) => string_table.resolve(*id).to_string(),

            TokenKind::Path(paths) => {
                let mut string = String::with_capacity(8 * paths.len());
                for path in paths {
                    string.extend(path.to_string(string_table).chars())
                }
                string
            }
            _ => String::new(),
        }
    }

    /// Compare this token's string content with a string slice efficiently.
    /// Only works for tokens that contain string data (Symbol, StringSliceLiteral, etc.).
    /// Returns false for tokens that don't contain string data.
    pub fn eq_str(&self, string_table: &StringTable, other: &str) -> bool {
        match &self.kind {
            TokenKind::Symbol(id)
            | TokenKind::StringSliceLiteral(id)
            | TokenKind::RawStringLiteral(id) => string_table.resolve(*id) == other,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileTokens {
    pub tokens: Vec<Token>,
    pub src_path: InternedPath,
    pub index: usize,
    pub length: usize,
}

impl FileTokens {
    pub fn new(src_path: InternedPath, tokens: Vec<Token>) -> FileTokens {
        FileTokens {
            length: tokens.len(),
            src_path,
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

    pub fn current_location(&self) -> TextLocation {
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
        while matches!(self.current_token_kind(), TokenKind::Newline) {
            self.index += 1;
        }
    }

    pub fn go_back(&mut self) {
        self.index -= 1;
    }

    // Used for header parsing
    // Or can be used for skipping an unused block of code
    // Assumes already inside a scope (have passed the first colon)
    pub fn skip_to_end_of_scope(&mut self) {
        let mut scopes_opened = 1;
        let mut scopes_closed = 0;

        while scopes_opened > scopes_closed {
            match self.current_token_kind() {
                TokenKind::End => scopes_closed += 1,
                TokenKind::Colon => scopes_opened += 1,
                TokenKind::Eof => break,
                _ => {}
            }
            self.advance();
        }
    }
}

pub struct TokenStream<'a> {
    pub file_path: &'a InternedPath,
    pub chars: Peekable<Chars<'a>>,
    pub position: CharPosition,
    pub start_position: CharPosition,
    pub mode: TokenizeMode,
}

impl<'a> TokenStream<'a> {
    pub fn new(source_code: &'a str, file_path: &'a InternedPath, mode: TokenizeMode) -> Self {
        Self {
            file_path,
            chars: source_code.chars().peekable(),
            position: CharPosition::default(),
            start_position: Default::default(),
            mode,
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

    pub fn new_location(&mut self) -> TextLocation {
        let start_pos = self.start_position;
        self.update_start_position();
        TextLocation::new(self.file_path.to_owned(), start_pos, self.position)
    }

    pub fn update_start_position(&mut self) {
        self.start_position = self.position;
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum TokenKind {
    // For Compiler
    ModuleStart, // Contains module name space
    Eof,         // End of the file

    // Module Import
    /// For Wasm files or host environment - importing from a different module or the host
    Export,

    /// For other Beanstalk files - indicates using public items from another file
    Use,

    // #
    Hash,

    /// Function Signatures
    Arrow,

    /// Variable name
    Symbol(StringId),

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
    EndTemplateHead,

    // Basic Grammar
    Comma,
    Dot,
    Colon,       // :
    DoubleColon, // ::
    Assign,      // =

    // Scope
    OpenParenthesis,  // (
    CloseParenthesis, // )

    As, // Type casting

    // Can modify types to become variadic parameters.
    // So any number of values can be passed in
    Variadic, // ..

    // Type Declarations
    Mutable,

    // Datatypes
    DatatypeNone,
    DatatypeInt,
    DatatypeFloat,
    DatatypeBool,
    DatatypeTrue,
    DatatypeFalse,
    DatatypeString,

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
    ElseIf,
    Loop,
    In,
    Break,
    Continue,
    Return,
    Defer,

    // Pattern matching
    Wildcard, // _
    Range,    // to

    // Memory Management
    Copy,

    // Templates
    EmptyTemplate(usize), // MIGHT REMOVE THIS
    Slot,
    TemplateClose,
    TemplateHead,

    Id(StringId), // ID for scenes

    Empty,

    // Channels
    CreateChannel,  // =>
    ChannelSend,    // >>
    ChannelReceive, // <<
    Yield,
}

impl TokenKind {
    pub fn get_name(&self, string_table: &StringTable) -> String {
        match self {
            TokenKind::Symbol(name) => string_table.resolve(*name).to_string(),
            TokenKind::RawStringLiteral(value) => string_table.resolve(*value).to_string(),
            TokenKind::StringSliceLiteral(string) => string_table.resolve(*string).to_string(),
            _ => String::new(),
        }
    }

    pub fn to_datatype(&self) -> Option<DataType> {
        match self {
            TokenKind::DatatypeInt => Some(DataType::Int),
            TokenKind::DatatypeFloat => Some(DataType::Float),
            TokenKind::DatatypeBool => Some(DataType::Bool),
            TokenKind::DatatypeString => Some(DataType::StringSlice),
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

// pub fn string_dimensions(s: &str) -> TokenLocation {
//     let (width, height) = s
//         .lines()
//         .map(|line| line.len())
//         .fold((0, 0), |(max_width, count), len| {
//             (max_width.max(len), count + 1)
//         });
//
//     TokenLocation {
//         line_number: height.max(1),
//         char_column: width as i32,
//     }
// }
