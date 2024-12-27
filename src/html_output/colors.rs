use crate::parsers::ast_nodes::Value;
use crate::{CompileError, Token};

// Returns the hsla value of the color in the color pallet
// Colors in Beanstalk can have shades between -100 and 100
/* TODO
    The color system will be overhauled completely to work around pallets and themes
*/
pub fn get_color(color: &Token, shade: &Value) -> Result<String, CompileError> {
    let mut transparency = 1.0;
    let param = match shade {
        Value::Int(value) => *value as f64,
        Value::Float(value) => *value,
        Value::Tuple(references) => {
            if references.len() > 2 {
                return Err(CompileError {
                    msg: "Error: Colors can only have a shade and a transparency value, more arguments provided".to_string(),
                    line_number: 0,
                });
            }
            transparency = match &references[1].value {
                Value::Int(value) => *value as f64 / 100.0,
                Value::Float(value) => *value,
                _ => 1.0,
            };
            match &references[0].value {
                Value::Int(value) => *value as f64,
                Value::Float(value) => *value,
                _ => 0.0,
            }
        }
        _ => 0.0,
    };

    let mut sat_param = param * -0.05;
    let mut lightness_param = param * 0.4;
    if param.is_sign_positive() {
        sat_param = param * 0.05;
        lightness_param = param * 0.15;
    }

    let saturation = 90.0 + sat_param;
    let lightness = 55.0 + lightness_param;

    Ok(match color {
        Token::Red => format!("{},{}%,{}%,{}", 0, saturation, lightness, transparency),
        Token::Orange => format!("{},{}%,{}%,{}", 25, saturation, lightness, transparency),
        Token::Yellow => format!("{},{}%,{}%,{}", 60, saturation, lightness, transparency),
        Token::Green => format!("{},{}%,{}%,{}", 120, saturation, lightness, transparency),
        Token::Cyan => format!("{},{}%,{}%,{}", 180, saturation, lightness, transparency),
        Token::Blue => format!("{},{}%,{}%,{}", 240, saturation, lightness, transparency),
        Token::Purple => format!("{},{}%,{}%,{}", 300, saturation, lightness, transparency),
        Token::Pink => format!("{},{}%,{}%,{}", 320, saturation, lightness, transparency),
        Token::White => format!("{},{}%,{}%,{}", 0, 0, 100, transparency),
        Token::Black => format!("{},{}%,{}%,{}", 0, 0, 0, transparency),
        _ => format!("{},{}%,{}%,{}", 0, 0, lightness, transparency),
    })
}
