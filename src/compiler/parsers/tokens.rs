use crate::compiler::datatypes::{DataType, Ownership};
use colour::red_ln;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Chars;

#[derive(Debug, PartialEq)]
pub enum TokenizeMode {
    Normal,
    TemplateBody,
    TemplateHead,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VarVisibility {
    // Default for anything not at the top level of a file
    Private,

    // Can be seen by other beanstalk files,
    // but not out of the resulting Wasm module
    // This is the default for any top level declarations in a file
    Public,

    // Exported out of the Wasm module
    Exported,
}

impl VarVisibility {
    pub fn is_private(&self) -> bool {
        matches!(self, VarVisibility::Private)
    }
    pub fn is_public(&self) -> bool {
        matches!(self, VarVisibility::Public)
    }
    pub fn is_exported(&self) -> bool {
        matches!(self, VarVisibility::Exported)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CharPosition {
    pub line_number: i32,
    pub char_column: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextLocation {
    pub scope: PathBuf,
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

impl TextLocation {
    pub fn new(scope: PathBuf, start: CharPosition, end: CharPosition) -> Self {
        Self {
            scope,
            start_pos: start,
            end_pos: end,
        }
    }

    pub fn new_same_line(&self, start: CharPosition, length: i32) -> Self {
        Self {
            scope: self.scope.to_owned(),
            start_pos: start,
            end_pos: CharPosition {
                line_number: start.line_number,
                char_column: start.char_column + length,
            },
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
}

#[derive(Clone)]
pub struct TokenContext {
    pub tokens: Vec<Token>,
    pub imports: HashSet<PathBuf>,
    pub src_path: PathBuf,
    pub index: usize,
    pub length: usize,
}

impl TokenContext {
    pub fn new(src_path: PathBuf, tokens: Vec<Token>, imports: HashSet<PathBuf>) -> TokenContext {
        TokenContext {
            length: tokens.len(),
            src_path,
            tokens,
            index: 0,
            imports,
        }
    }

    pub fn current_token_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
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
        match &self.current_token_kind() {
            // Some tokens allow any number of newlines after them,
            // without breaking a statement or expression
            &TokenKind::Colon
            | &TokenKind::OpenParenthesis
            | &TokenKind::StructBracket
            | &TokenKind::Comma
            | &TokenKind::End
            | &TokenKind::Assign
            | &TokenKind::AddAssign
            | &TokenKind::SubtractAssign
            | &TokenKind::MultiplyAssign
            | &TokenKind::DivideAssign
            | &TokenKind::ExponentAssign
            | &TokenKind::RootAssign
            | &TokenKind::Add
            | &TokenKind::Subtract
            | &TokenKind::Multiply
            | &TokenKind::Divide
            | &TokenKind::Modulus
            | &TokenKind::Root
            | &TokenKind::Arrow
            | &TokenKind::Is
            | &TokenKind::LessThan
            | &TokenKind::LessThanOrEqual
            | &TokenKind::GreaterThan
            | &TokenKind::GreaterThanOrEqual => {
                self.index += 1;
                self.skip_newlines();
            }

            // Can't advance past End of File
            &TokenKind::Eof => {
                // Show a warning for compiler development purposes
                #[cfg(feature = "show_tokens")]
                red_ln!("Compiler tried to advance past EOF");
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
}

pub struct TokenStream<'a> {
    pub file_path: &'a Path,
    pub chars: Peekable<Chars<'a>>,
    pub position: CharPosition,
    pub start_position: CharPosition,
    pub context: TokenizeMode,
}

impl<'a> TokenStream<'a> {
    pub fn new(source_code: &'a str, file_path: &'a Path) -> Self {
        Self {
            file_path,
            chars: source_code.chars().peekable(),
            position: CharPosition::default(),
            start_position: Default::default(),
            context: TokenizeMode::Normal,
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

        TextLocation::new(self.file_path.to_path_buf(), start_pos, self.position)
    }

    pub fn update_start_position(&mut self) {
        self.start_position = self.position;
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum TokenKind {
    // For Compiler
    ModuleStart(String), // Contains module name space
    Eof,                 // End of the file

    // Module Import/Export
    Import, // Directed through a different path so not needed after tokenizer.

    // For exporting functions or variables outside the final module.
    // Top level declarations are public to the project automatically,
    // So this has nothing to do with internal visibility.
    Export,

    // HTML project compiler directives
    Print,
    Log,
    Panic,
    Assert,

    Comptime,
    Settings,
    Page,
    Component,
    Title,
    Date,
    Wat(String), // WAT codeblock (for testing WASM)

    // Scene Style properties
    Markdown,     // Makes the scene Markdown
    ChildDefault, // This scene will become a template default for all child scenes of the parent
    Ignore,       // for commenting out an entire scene
    CodeKeyword,

    // Standard Library (eventually - to be moved there)
    Math,

    // Variables / Functions
    Arrow,

    Symbol(String),

    // Literals
    StringLiteral(String),
    PathLiteral(PathBuf),
    FloatLiteral(f64),
    IntLiteral(i64),
    CharLiteral(char),
    RawStringLiteral(String),
    BoolLiteral(bool),

    // Collections
    OpenCurly,  // {
    CloseCurly, // }

    StructBracket, // |

    // Structure of Syntax
    Newline,
    End,

    // Basic Grammar
    Comma,
    Dot,
    Colon,  // :
    Assign, // =

    // Scope
    OpenParenthesis,  // (
    CloseParenthesis, // )
    TemplateOpen,     // [
    TemplateClose,    // Used to track of the spaces following the scene, not needed now?

    As, // Type casting

    // Type Declarations
    Mutable,
    Choice, // ::

    // Datatypes
    DatatypeInt,
    DatatypeFloat,
    DatatypeBool,
    DatatypeString,
    DatatypeTemplate,
    DatatypeNone,

    Async,

    Bang,
    QuestionMark,

    //Mathematical Operators
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
    If,
    Else,
    ElseIf,
    For,
    In,
    Break,
    Continue,
    Return,
    Defer,

    // Memory Management
    Copy,

    // Templates
    ParentTemplate,
    EmptyTemplate(usize), // Used for templating values in scene heads in the body of scenes, value is number of spaces after the scene template
    Slot,
    TemplateHead,

    Id(String), // ID for scenes

    Empty,
    // Pre(String), // Content inside raw elements. Might change to not be a format tag in the future
}

impl TokenKind {
    pub fn get_name(&self) -> String {
        match self {
            TokenKind::Symbol(name, ..) => name.clone(),
            TokenKind::RawStringLiteral(value) => value.clone(),
            TokenKind::StringLiteral(string) => string.clone(),
            TokenKind::ModuleStart(name) => name.clone(),
            _ => String::new(),
        }
    }

    pub fn to_datatype(&self, ownership: Ownership) -> Option<DataType> {
        match self {
            TokenKind::DatatypeInt => Some(DataType::Int(ownership)),
            TokenKind::DatatypeFloat => Some(DataType::Float(ownership)),
            TokenKind::DatatypeBool => Some(DataType::Bool(ownership)),
            TokenKind::DatatypeString => Some(DataType::String(ownership)),
            TokenKind::DatatypeTemplate => Some(DataType::Template(ownership)),
            _ => None,
        }
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
