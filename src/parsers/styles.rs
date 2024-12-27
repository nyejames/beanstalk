use crate::Token;

use super::ast_nodes::Value;

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
    Redirect(Value, u32), // src

    // HTML tags
    Span,
    Div,
    P, // To check whether scene is already inside a P tag
    Heading,
    BulletPoint,
    Em,
    Superscript,
    A(Value, u32),     // src
    Img(Value, u32),   // src
    Video(Value, u32), // src
    Audio(Value, u32), // src
    Table(Value, u32), // Columns
    Code(String, u32), // Language

    Nav(Value, u32), // different nav styles
    List,

    // Custom Beanstalk Tags
    Title(Value, u32),

    Button(Value, u32), // Different button styles
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Click(Value, u32),
    _Swap,
}

// Will contain an expression or collection of expressions to be parsed in the target language
#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    Padding(Value, u32),
    Margin(Value, u32),
    Size(Value, u32), // Size of text

    // Colours keywords = -100 to 100 as different shades. -100 darkest, 100 lightest
    TextColor(Value, Token, u32), // Value, type (rgb, hsl)
    BackgroundColor(Value, u32),
    Alt(String, u32),
    Center(bool, u32), // true = also center vertically
    Order(Value, u32), // For positioning elements inside a grid/flex container/nav etc
    Hide(u32),
    Blank,
}
