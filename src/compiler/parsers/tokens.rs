use crate::compiler::datatypes::DataType;
use colour::red_ln;
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;

#[derive(Debug, PartialEq)]
pub enum TokenizeMode {
    Normal,
    SceneBody,
    SceneHead,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VarVisibility {
    // Default
    Private,

    // Can be seen by other beanstalk files,
    // but not out of the resulting Wasm module
    Public,

    // Exported out of the Wasm module
    Exported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CharPosition {
    pub line_number: i32,
    pub char_column: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextLocation {
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

impl TextLocation {
    pub fn new(start: CharPosition, end: CharPosition) -> Self {
        Self {
            start_pos: start,
            end_pos: end,
        }
    }

    pub fn new_same_line(start: CharPosition, length: i32) -> Self {
        Self {
            start_pos: start,
            end_pos: CharPosition {
                line_number: start.line_number,
                char_column: start.char_column + length,
            },
        }
    }
}

#[derive(Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub location: TextLocation,
}

impl Token {
    pub fn new(kind: TokenKind, location: TextLocation) -> Self {
        Self { kind, location }
    }

    pub fn new_same_line(kind: TokenKind, start: CharPosition, length: i32) -> Self {
        Self {
            kind,
            location: TextLocation::new_same_line(start, length),
        }
    }

    pub fn to_string(&self) -> String {
        format!("{:?}", self.kind)
    }
}

pub struct TokenContext {
    pub tokens: Vec<Token>,
    pub index: usize,
    pub length: usize,
}

impl TokenContext {
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
        self.tokens[self.index].location
    }

    pub fn advance(&mut self) {
        match &self.current_token_kind() {
            // Some tokens allow any number of newlines after them,
            // without breaking a statement or expression
            &TokenKind::Colon
            | &TokenKind::OpenParenthesis
            | &TokenKind::StructDefinition
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
            &TokenKind::EOF => {
                // Show a warning for compiler development purposes
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
    pub chars: Peekable<Chars<'a>>,
    pub position: CharPosition,
    pub start_position: CharPosition,
    pub context: TokenizeMode,
}

impl<'a> TokenStream<'a> {
    pub fn new(source_code: &'a str) -> Self {
        Self {
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

        TextLocation::new(start_pos, self.position)
    }

    pub fn update_start_position(&mut self) {
        self.start_position = self.position;
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum TokenKind {
    // For Compiler
    ModuleStart(String), // Contains module name space
    EOF,                 // End of the file

    // Module Import/Export
    Import(String),

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
    JS(String),   // JS codeblock
    CSS(String),  // CSS codeblock
    WASM(String), // WAT codeblock (for testing WASM)

    // Scene Style properties
    Markdown,     // Makes the scene Markdown
    ChildDefault, // This scene will become a template default for all child scenes of the parent
    Ignore,       // for commenting out an entire scene
    CodeKeyword,

    // Standard Library (eventually - to be moved there)
    Math,

    // Comments
    Comment,

    // Variables / Functions
    Arrow,

    Symbol(String),

    // Literals
    StringLiteral(String),
    PathLiteral(PathBuf),
    FloatLiteral(f64),
    IntLiteral(i32),
    CharLiteral(char),
    RawStringLiteral(String),
    BoolLiteral(bool),

    // Collections
    OpenCurly,  // {
    CloseCurly, // }

    StructDefinition, // |

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
    SceneOpen,        // [
    SceneClose,       // Used to track of the spaces following the scene, not needed now?

    As, // Type casting

    // Type Declarations
    Mutable,
    Choice, // ::
    DatatypeLiteral(DataType),

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

    // Scenes
    ParentScene,
    EmptyScene(usize), // Used for templating values in scene heads in the body of scenes, value is number of spaces after the scene template
    Slot,
    SceneHead,

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
