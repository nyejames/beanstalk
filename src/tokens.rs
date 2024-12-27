use std::path::PathBuf;
use crate::bs_types::DataType;

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
    Comptime,
    Error(String, u32),  // Error message, line number
    DeadVariable(String), // Name. Variable that is never used, to be removed in the AST
    EOF,                 // End of file

    // Module Import/Export
    Import,
    Use,
    Export,

    // HTML project compiler directives
    Page,
    Component,
    Title,
    Date,
    JS(String),  // JS codeblock
    CSS(String), // CSS codeblock
    WASM(String), // WAT codeblock (for testing WASM)

    // Standard Library (eventually)
    Settings,
    Print,
    Math,

    // Comments
    Comment(String),
    DocComment(String),

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

    // Errors
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
    Continue, // Might also operate as a fallthrough operator
    Return,
    End,
    Defer,
    Assert,
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
    A,   // href, content
    Img, // src, alt
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
