use super::ast_nodes::Value;
use crate::tokenizer::TokenPosition;
use crate::Token;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Tag {
    None,
    Id(Value),

    // Structure of the page
    Main,
    Header,
    Footer,
    Section,

    // Scripts
    Redirect(Value, TokenPosition), // src

    // HTML tags
    Span,
    Div,
    P, // To check whether scene is already inside a P tag
    Heading,
    BulletPoint,
    Em,
    Superscript,
    A(Value, TokenPosition),     // src
    Img(Value, TokenPosition),   // src
    Video(Value, TokenPosition), // src
    Audio(Value, TokenPosition), // src
    Table(Value, TokenPosition), // Columns
    Code(String, TokenPosition), // Language

    Nav(Value, TokenPosition), // different nav styles
    List,

    // Custom Beanstalk Tags
    Title(Value, TokenPosition),

    Button(Value, TokenPosition), // Different button styles
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Click(Value, TokenPosition),
    _Swap,
}

// Will contain an expression or collection of expressions to be parsed in the target language
#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    Padding(Value, TokenPosition),
    Margin(Value, TokenPosition),
    Size(Value, TokenPosition), // Size of text

    // Colours keywords = -100 to 100 as different shades. -100 darkest, 100 lightest
    TextColor(Value, Token, TokenPosition), // Value, type (rgb, hsl)
    BackgroundColor(Value, TokenPosition),
    Alt(String, TokenPosition),
    Center(bool, TokenPosition), // true = also center vertically
    Order(Value, TokenPosition), // For positioning elements inside a grid/flex container/nav etc
    Hide(TokenPosition),
    Blank,
}
