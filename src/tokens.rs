use crate::bs_types::{get_type_keyword_length, DataType};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum TokenizeMode {
    Normal,
    Markdown,
    Codeblock,
    SceneHead,
    CompilerDirective, // #
}

#[derive(PartialEq, Debug, Clone)]
pub enum Token {
    // For Compiler
    ModuleStart(String),
    Print,
    DeadVariable(String), // Name. Variable that is never used, to be removed in the AST
    EOF,                  // End of file

    // Module Import/Export
    Import,
    Use,
    Public,

    // HTML project compiler directives
    Comptime,
    Settings,
    Page,
    Component,
    Title,
    Date,
    JS(String),   // JS codeblock
    CSS(String),  // CSS codeblock
    WASM(String), // WAT codeblock (for testing WASM)

    // Standard Library (eventually - to be moved there)
    Math,

    // Comments
    Comment(String),

    // Variables / Functions
    Arrow,
    Variable(String),

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

    // Structure of Syntax
    Newline,
    Semicolon,

    // Basic Grammar
    Comma,
    Dot,
    Colon,  // :
    Assign, // =

    // Scope
    OpenParenthesis,  // (
    CloseParenthesis, // )
    SceneOpen,        // [
    SceneClose(u32),  // Keeps track of the spaces following the scene

    As, // Type casting

    // Type Declarations
    TypeKeyword(DataType),

    FunctionKeyword,

    // Result Type / Option Type
    Bang,
    QuestionMark,

    //Mathematical Operators in order of precedence
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
    Equal,
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
    End,
    Defer,
    Assert,
    
    // Memory Management
    Copy,

    // Scenes
    ParentScene,
    EmptyScene(u32), // Used for templating values in scene heads in the body of scenes, value is number of spaces after the scene template

    SceneHead,
    SceneBody,
    Signal(String),

    // HTTP
    Dollar,

    //HTML element stuff
    //markdown inferred elements
    Id,
    Span(String),
    P(String),
    Em(u8, String), // Forms the start and the end of an Em tag
    Superscript(String),
    HeadingStart(u8), // Max heading size should be 10 or something
    BulletPointStart(u8),
    Empty,
    Pre(String), // Content inside raw elements. Might change to not be a format tag in the future

    Ignore, // for commenting out an entire scene

    // named tags
    Link, // href, content
    Img,  // src, alt
    Video,
    Audio,
    Raw,

    Alt,

    // Styles
    Padding,
    Margin,
    Size,
    Rgb,
    Hsv,
    Hsl,
    BG,
    Table,
    Center,
    CodeKeyword,
    CodeBlock(String), // Content, Language
    Order,
    Blank,
    Hide,

    // Colours
    Color,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    White,
    Black,
    Orange,
    Pink,
    Purple,
    Grey,

    // Structure of the page
    Main,
    Header,
    Footer,
    Section,
    Gap,

    Nav,
    Button,
    Canvas,
    Click,
    Form,
    Option,
    Dropdown,
    Input,
    Redirect,
}

pub trait Length {
    fn length(&self) -> u32;
}

impl Length for Token {
    fn length(&self) -> u32 {
        match self {
            // Might change
            Token::FunctionKeyword => 2,
            Token::Settings => 7,
            Token::Math => 4,
            Token::Page => 4,
            Token::Component => 8,
            Token::Title => 5,
            Token::Date => 4,

            Token::Print => 5,
            Token::ModuleStart(_) => 0,
            Token::Newline => 0,
            Token::EOF => 0,
            Token::Import => 6,
            Token::Use => 3,

            Token::Variable(name) => name.len() as u32,
            Token::DeadVariable(name) => name.len() as u32,
            Token::JS(code) => code.len() as u32,
            Token::CSS(code) => code.len() as u32,
            Token::WASM(code) => code.len() as u32,
            Token::Comment(content) => content.len() as u32,

            Token::TypeKeyword(data_type) => get_type_keyword_length(data_type),

            Token::StringLiteral(string) => string.len() as u32 + 2,
            Token::PathLiteral(path) => path.to_string_lossy().len() as u32,
            Token::FloatLiteral(value) => value.to_string().len() as u32,
            Token::IntLiteral(value) => value.to_string().len() as u32,
            Token::CharLiteral(value) => value.len_utf8() as u32 + 2,
            Token::RawStringLiteral(value) => value.len() as u32,
            Token::BoolLiteral(value) => value.to_string().len() as u32,

            Token::As => 2,
            Token::Arrow => 2,

            Token::ExponentAssign => 2,
            Token::MultiplyAssign => 2,
            Token::DivideAssign => 2,
            Token::Remainder => 2,
            Token::RemainderAssign => 2,
            Token::Root => 2,
            Token::ModulusAssign => 2,
            Token::RootAssign => 2,

            Token::AddAssign => 2,
            Token::SubtractAssign => 2,

            Token::LessThanOrEqual => 2,
            Token::GreaterThanOrEqual => 2,

            Token::And => 3,
            Token::Or => 2,
            Token::Not => 3,
            Token::Equal => 2,

            Token::If => 2,
            Token::Else => 4,
            Token::ElseIf => 5,
            Token::For => 3,
            Token::In => 2,
            Token::Break => 4,
            Token::Continue => 7,
            Token::Return => 5,
            Token::End => 3,
            Token::Defer => 5,
            Token::Assert => 6,
            Token::Copy => 4,

            Token::EmptyScene(_) => 0,
            Token::Signal(value) => value.len() as u32,

            Token::Span(content) => content.len() as u32,
            Token::P(content) => content.len() as u32,
            Token::Em(_, content) => content.len() as u32,
            Token::Superscript(content) => content.len() as u32,
            Token::HeadingStart(strength) => *strength as u32,
            Token::BulletPointStart(strength) => *strength as u32,
            Token::Empty => 0,
            Token::Pre(content) => content.len() as u32,
            Token::Ignore => 0,

            Token::Link => 4,
            Token::Img => 3,
            Token::Video => 4,
            Token::Audio => 4,
            Token::Raw => 3,
            Token::Alt => 3,

            Token::Padding => 6,
            Token::Margin => 5,
            Token::Size => 4,
            Token::Rgb => 3,
            Token::Hsv => 3,
            Token::Hsl => 3,
            Token::BG => 2,
            Token::Table => 5,
            Token::Center => 6,
            Token::CodeKeyword => 6,
            Token::CodeBlock(content) => content.len() as u32,
            Token::Order => 5,
            Token::Blank => 5,
            Token::Hide => 4,

            Token::Color => 5,
            Token::Red => 3,
            Token::Green => 4,
            Token::Blue => 4,
            Token::Yellow => 5,
            Token::Cyan => 4,
            Token::Magenta => 6,
            Token::White => 5,
            Token::Black => 4,
            Token::Orange => 5,
            Token::Pink => 4,
            Token::Purple => 5,
            Token::Grey => 4,

            Token::Main => 4,
            Token::Header => 6,
            Token::Footer => 6,
            Token::Section => 7,
            Token::Gap => 4,

            Token::Nav => 3,
            Token::Button => 6,
            Token::Canvas => 6,
            Token::Click => 5,
            Token::Form => 4,
            Token::Option => 5,
            Token::Dropdown => 7,
            Token::Input => 5,
            Token::Redirect => 5,

            _ => 1,
        }
    }
}
