// use crate::headers::scene::{PrecedenceStyle, Style, StyleFormat};
// use std::collections::HashMap;
//
// // Core HTML Styles
//
// // The structure of the opening tag
// enum BeforeIndex {
//     TagName1,
//     ClassOpen,
//     Classes,
//     StringClose1,
//     StyleStart,
//     Style,
//     StringClose2,
//     AltOpen,
//     Alt,
//     StringClose3,
//     CloseFirstTag,
// }
//
// const TEMPLATE_BEFORE_WRAPPER: [&str; 11] = [
//     "",
//     " class=\"",
//     "",
//     "\"",
//     " style=\"",
//     "",
//     "\"",
//     " alt=\"",
//     "",
//     "\"",
//     ">",
// ];
//
// // CORE TOP LEVEL STYLES
//
// // TODO: rewrite this in beanstalk itself
// // Styles will just be regular structs that export certain style fields
// // Templates inside those structs will have slots instead of this wrapper thing
// pub fn get_html_styles() -> [(String, Style); 4] {
//     // THE CORE STYLES
//
//     // PAGE
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<main");
//     before[BeforeIndex::Classes as usize].string = String::from("container");
//     let page = Style {
//         wrapper: Wrapper {
//             before,
//             after: Vec::from([WrapperString {
//                 string: String::from("</main>"),
//                 ..WrapperString::default()
//             }]),
//         },
//         format: StyleFormat::Markdown as i32,
//         unlocked_styles: HashMap::from(get_basic_unlockable_styles()),
//         child_default: Some(Box::new(PrecedenceStyle {
//             style: Style {
//                 format: StyleFormat::Markdown as i32,
//                 ..Style::default()
//             },
//             precedence: -1,
//         })),
//         ..Style::default()
//     };
//
//     // NAVBAR
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<nav");
//     before[BeforeIndex::Classes as usize].string = String::from("bs-nav-0 ");
//     let navbar = Style {
//         wrapper: Wrapper {
//             before,
//             after: Vec::from([WrapperString {
//                 string: String::from("</nav>"),
//                 ..WrapperString::default()
//             }]),
//         },
//         format: StyleFormat::Markdown as i32,
//         unlocked_styles: HashMap::from(get_basic_unlockable_styles().to_owned()),
//         child_default: Some(Box::new(PrecedenceStyle {
//             style: Style {
//                 format: StyleFormat::Markdown as i32,
//                 ..Style::default()
//             },
//             precedence: -1,
//         })),
//         ..Style::default()
//     };
//
//     // HEADER
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<header");
//     before[BeforeIndex::Classes as usize].string = String::from("container ");
//     let header = Style {
//         wrapper: Wrapper {
//             before,
//             after: Vec::from([WrapperString {
//                 string: String::from("</header>"),
//                 ..WrapperString::default()
//             }]),
//         },
//         format: StyleFormat::Markdown as i32,
//         unlocked_styles: HashMap::from(get_basic_unlockable_styles().to_owned()),
//         child_default: Some(Box::new(PrecedenceStyle {
//             style: Style {
//                 format: StyleFormat::Markdown as i32,
//                 ..Style::default()
//             },
//             precedence: -1,
//         })),
//         ..Style::default()
//     };
//
//     // FOOTER
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<footer");
//     before[BeforeIndex::Classes as usize].string = String::from("container ");
//     let footer = Style {
//         wrapper: Wrapper {
//             before,
//             after: Vec::from([WrapperString {
//                 string: String::from("</footer>"),
//                 ..WrapperString::default()
//             }]),
//         },
//         format: StyleFormat::Markdown as i32,
//         unlocked_styles: HashMap::from(get_basic_unlockable_styles().to_owned()),
//         child_default: Some(Box::new(PrecedenceStyle {
//             style: Style {
//                 format: StyleFormat::Markdown as i32,
//                 ..Style::default()
//             },
//             precedence: -1,
//         })),
//         ..Style::default()
//     };
//
//     // Component
//     // Canvas
//     // App
//
//     // These have PascalCase names as they are top level Styles
//     [
//         ("Page".to_string(), page),
//         ("Navbar".to_string(), navbar),
//         ("Header".to_string(), header),
//         ("Footer".to_string(), footer),
//         // More here like Section, Component, App, etc..
//     ]
// }
//
// fn get_basic_unlockable_styles() -> [(String, Style); 4] {
//     // UNLOCKABLE STYLES INSIDE CORE STYLES
//
//     // Link, // href, content
//     // Img,  // src, alt
//     // Video,
//     // Audio,
//     // Raw,
//     // Alt,
//     // Styles
//     // Padding,
//     // Margin,
//     // Size,
//     // Rgb,
//     // Hsv,
//     // Hsl,
//     // BG,
//     // Table,
//     // Center,
//
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::Classes as usize].string = String::from("bs-center ");
//     let center = (
//         "center".to_string(),
//         Style {
//             wrapper: Wrapper {
//                 before,
//                 after: Vec::new(),
//             },
//             format: StyleFormat::Markdown as i32,
//             ..Style::default()
//         },
//     );
//
//     // Order,
//     // Blank,
//     // Hide,
//
//     // Gap,
//     // Button,
//
//     // Click,
//     // Form,
//     // Option,
//     // Dropdown,
//     // Input,
//     // Redirect,
//
//     // Colors
//
//     // red
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<span");
//     before[BeforeIndex::Style as usize].string = String::from("color:red;");
//     let red = (
//         "red".to_string(),
//         Style {
//             wrapper: Wrapper {
//                 before,
//                 after: Vec::new(),
//             },
//             format: StyleFormat::Markdown as i32,
//             ..Style::default()
//         },
//     );
//
//     // green
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<span");
//     before[BeforeIndex::Style as usize].string = String::from("color:green;");
//     let green = (
//         "green".to_string(),
//         Style {
//             wrapper: Wrapper {
//                 before,
//                 after: Vec::new(),
//             },
//             format: StyleFormat::Markdown as i32,
//             ..Style::default()
//         },
//     );
//
//     // blue
//     let mut before = get_basic_wrapper();
//     before[BeforeIndex::TagName1 as usize].string = String::from("<span");
//     before[BeforeIndex::Style as usize].string = String::from("color:blue;");
//     let blue = (
//         "blue".to_string(),
//         Style {
//             wrapper: Wrapper {
//                 before,
//                 after: Vec::from([WrapperString {
//                     string: String::from("</span>"),
//                     ..WrapperString::default()
//                 }]),
//             },
//             format: StyleFormat::Markdown as i32,
//             ..Style::default()
//         },
//     );
//     // Yellow,
//     // Cyan,
//     // Magenta,
//     // White,
//     // Black,
//     // Orange,
//     // Pink,
//     // Purple,
//     // Grey,
//
//     [red, green, blue, center]
// }
//
// fn get_basic_wrapper() -> Vec<WrapperString> {
//     let basic_wrapper: Vec<WrapperString> = TEMPLATE_BEFORE_WRAPPER
//         .iter()
//         .map(|s| WrapperString {
//             string: s.to_string(),
//             groups: &[],
//             incompatible_groups: &[],
//             required_groups: &[],
//             overwrite: false,
//         })
//         .collect();
//
//     basic_wrapper.to_owned()
// }
//
// // OLD
//
// // #[derive(Debug, Clone, PartialEq)]
// // #[allow(dead_code)]
// // pub enum Tag {
// //     None,
// //     Id(Value),
// //
// //     // Structure of the page
// //     Main,
// //     Header,
// //     Footer,
// //     Section,
// //
// //     // Scripts
// //     Redirect(Value, TokenPosition), // src
// //
// //     // HTML tags
// //     Span,
// //     Div,
// //     P, // To check whether scene is already inside a P tag
// //     Heading,
// //     BulletPoint,
// //     Em,
// //     Superscript,
// //     A(Value, TokenPosition),     // src
// //     Img(Value, TokenPosition),   // src
// //     Video(Value, TokenPosition), // src
// //     Audio(Value, TokenPosition), // src
// //     Table(Value, TokenPosition), // Columns
// //     Code(String, TokenPosition), // Language
// //
// //     Nav(Value, TokenPosition), // different nav styles
// //     List,
// //
// //     // Custom Beanstalk Tags
// //     Title(Value, TokenPosition),
// //
// //     Button(Value, TokenPosition), // Different button styles
// // }
//
// // #[derive(Debug, Clone, PartialEq)]
// // pub enum Action {
// //     Click(Value, TokenPosition),
// //     _Swap,
// // }
// //
// // // Will contain an expression or collection of expressions to be parsed in the target language
// // #[derive(Debug, Clone, PartialEq)]
// // pub enum Style {
// //     Padding(Value, TokenPosition),
// //     Margin(Value, TokenPosition),
// //     Size(Value, TokenPosition), // Size of text
// //
// //     // Colours keywords = -100 to 100 as different shades. -100 darkest, 100 lightest
// //     TextColor(Value, Token, TokenPosition), // Value, type (rgb, hsl)
// //     BackgroundColor(Value, TokenPosition),
// //     Alt(String, TokenPosition),
// //     Center(bool, TokenPosition), // true = also center vertically
// //     Order(Value, TokenPosition), // For positioning elements inside a grid/flex container/nav etc
// //     Hide(TokenPosition),
// //     Blank,
// // }
