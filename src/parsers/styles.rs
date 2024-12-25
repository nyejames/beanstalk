use crate::Token;

use super::ast_nodes::{Arg, AstNode};

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Tag {
    None,
    Id(Vec<Arg>),

    // Structure of the page
    Main,
    Header,
    Footer,
    Section,

    // Scripts
    Redirect(AstNode), // src

    // HTML tags
    Span,
    Div,
    P, // To check whether scene is already inside a P tag
    Heading,
    BulletPoint,
    Em,
    Superscript,
    A(AstNode),     // src
    Img(AstNode),   // src
    Video(AstNode), // src
    Audio(AstNode), // src
    Table(u32),
    Code(String), // Language

    Nav(f64), // different nav styles
    List,

    // Custom Beanstalk Tags
    Title(AstNode),

    Button(AstNode), // Different button styles
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Click(AstNode),
    _Swap,
}

// Will contain an expression or collection of expressions to be parsed in the target language
#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    Padding(AstNode),
    Margin(AstNode),
    Size(AstNode), // Size of text

    // Colours keywords = -100 to 100 as different shades. -100 darkest, 100 lightest
    TextColor(AstNode, Token), // Value, type (rgb, hsl)
    BackgroundColor(AstNode),
    Alt(String),
    Center(bool),   // true = also center vertically
    Order(AstNode), // For positioning elements inside a grid/flex container/nav etc
    Hide,
    Blank,
}
