use crate::bs_types::DataType;
use crate::tokenizer::TokenPosition;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum TokenizeMode {
    Normal,
    SceneBody,
    SceneHead,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VarVisibility {
    Temporary,
    Public,
    Private,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Token {
    // For Compiler
    ModuleStart(String),  // Contains module name space
    DeadVariable(String), // Name. Variable that is never used, to be removed in the AST
    EOF,                  // End of the file

    // Module Import/Export
    Import,
    From,

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
    Comment(String),

    // Variables / Functions
    Arrow,

    // Mutability modifier here is only if this is used as a reference,
    // The type of a new variable declaration will otherwise carry the info about it's mutability
    Variable(String, VarVisibility, bool), // name, visibility, mutability (if passed as a reference)

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

    ArgConstructor, // |

    // Structure of Syntax
    Newline,
    End,
    Semicolon, // Might not be used at all in the language

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
    Public,
    Private,
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
    Then,
    Defer,

    // Memory Management
    Copy,

    // Scenes
    ParentScene,
    EmptyScene(usize), // Used for templating values in scene heads in the body of scenes, value is number of spaces after the scene template
    Slot,
    SceneHead,
    Dolla,

    Id(String), // ID for scenes

    Empty,
    // Pre(String), // Content inside raw elements. Might change to not be a format tag in the future
}

impl Token {
    pub fn dimensions(&self) -> TokenPosition {
        match self {
            // Might change
            Token::Settings => TokenPosition {
                line_number: 0,
                char_column: 7,
            },
            Token::Math => TokenPosition {
                line_number: 0,
                char_column: 4,
            },
            Token::Page => TokenPosition {
                line_number: 0,
                char_column: 4,
            },
            Token::Component => TokenPosition {
                line_number: 0,
                char_column: 8,
            },
            Token::Title => TokenPosition {
                line_number: 0,
                char_column: 5,
            },
            Token::Date => TokenPosition {
                line_number: 0,
                char_column: 4,
            },

            Token::Print => TokenPosition {
                line_number: 0,
                char_column: 5,
            },

            Token::ModuleStart(..) => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

            Token::Newline => TokenPosition {
                line_number: 1,
                char_column: 0,
            },

            Token::EOF => TokenPosition::default(),
            Token::Import => TokenPosition {
                line_number: 0,
                char_column: 5,
            },

            Token::Variable(name, ..) => TokenPosition {
                line_number: 0,
                char_column: name.len() as i32,
            },
            Token::DeadVariable(name) => TokenPosition {
                line_number: 0,
                char_column: name.len() as i32,
            },
            Token::JS(code) => string_dimensions(code),
            Token::CSS(code) => string_dimensions(code),
            Token::WASM(code) => string_dimensions(code),
            Token::Comment(content) => string_dimensions(content),

            Token::DatatypeLiteral(data_type) => TokenPosition {
                line_number: 0,
                char_column: data_type.length() as i32,
            },

            Token::StringLiteral(string) => string_dimensions(string),
            Token::PathLiteral(path) => string_dimensions(&path.to_string_lossy()),
            Token::FloatLiteral(value) => TokenPosition {
                line_number: 0,
                char_column: value.to_string().len() as i32,
            },
            Token::IntLiteral(value) => TokenPosition {
                line_number: 0,
                char_column: value.to_string().len() as i32,
            },
            Token::CharLiteral(value) => TokenPosition {
                line_number: 0,
                char_column: value.len_utf8() as i32 + 2,
            },
            Token::RawStringLiteral(value) => string_dimensions(value),
            Token::BoolLiteral(value) => TokenPosition {
                line_number: 0,
                char_column: value.to_string().len() as i32,
            },

            Token::And => TokenPosition {
                line_number: 0,
                char_column: 3,
            },
            Token::Not => TokenPosition {
                line_number: 0,
                char_column: 3,
            },
            Token::Else => TokenPosition {
                line_number: 0,
                char_column: 4,
            },
            Token::ElseIf => TokenPosition {
                line_number: 0,
                char_column: 5,
            },
            Token::For => TokenPosition {
                line_number: 0,
                char_column: 3,
            },
            Token::Break => TokenPosition {
                line_number: 0,
                char_column: 4,
            },
            Token::Continue => TokenPosition {
                line_number: 0,
                char_column: 7,
            },
            Token::Return => TokenPosition {
                line_number: 0,
                char_column: 5,
            },
            Token::End => TokenPosition {
                line_number: 0,
                char_column: 3,
            },
            Token::Defer => TokenPosition {
                line_number: 0,
                char_column: 5,
            },
            Token::Assert => TokenPosition {
                line_number: 0,
                char_column: 6,
            },
            Token::Copy => TokenPosition {
                line_number: 0,
                char_column: 4,
            },

            Token::EmptyScene(_) => TokenPosition::default(),
            Token::Dolla => TokenPosition {
                line_number: 0,
                char_column: 1,
            },

            Token::Ignore => TokenPosition {
                line_number: 0,
                char_column: 6,
            },

            // most stuff is 2 characters long
            _ => TokenPosition {
                line_number: 0,
                char_column: 2,
            },
        }
    }

    pub fn get_name(&self) -> String {
        match self {
            Token::Variable(name, ..) => name.clone(),
            Token::DeadVariable(name) => name.clone(),
            Token::Comment(content) => content.clone(),
            Token::RawStringLiteral(value) => value.clone(),
            Token::StringLiteral(string) => string.clone(),
            Token::ModuleStart(name) => name.clone(),
            _ => String::new(),
        }
    }
}

pub fn string_dimensions(s: &str) -> TokenPosition {
    let (width, height) = s
        .lines()
        .map(|line| line.len())
        .fold((0, 0), |(max_width, count), len| {
            (max_width.max(len), count + 1)
        });

    TokenPosition {
        line_number: height.max(1),
        char_column: width as i32,
    }
}
