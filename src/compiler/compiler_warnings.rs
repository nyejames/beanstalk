use crate::compiler::parsers::tokens::TextLocation;
use colour::yellow_ln_bold;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CompilerWarning {
    pub msg: String,
    pub location: TextLocation,
    pub warning_kind: WarningKind,
    pub file_path: PathBuf,
}

impl CompilerWarning {
    pub fn new(
        msg: &str,
        location: TextLocation,
        warning_kind: WarningKind,
        file_path: PathBuf,
    ) -> CompilerWarning {
        CompilerWarning {
            msg: msg.to_owned(),
            location,
            warning_kind,
            file_path,
        }
    }
}

#[derive(Clone, Debug)]
pub enum WarningKind {
    UnusedVariable,
    UnusedFunction,
    UnusedImport,
    UnusedType,
    UnusedConstant,
    UnusedFunctionArgument,
    UnusedFunctionReturnValue,
    UnusedFunctionParameter,
    UnusedFunctionParameterDefaultValue,
    PointlessExport,
}

pub fn print_formatted_warning(w: CompilerWarning) {
    yellow_ln_bold!("WARNING: ");
    match w.warning_kind {
        WarningKind::UnusedVariable => {
            println!("Unused variable '{}'", w.msg);
        }
        WarningKind::UnusedFunction => {
            println!("Unused function '{}'", w.msg);
        }
        WarningKind::UnusedImport => {
            println!("Unused import '{}'", w.msg);
        }
        WarningKind::UnusedType => {
            println!("Unused type '{}'", w.msg);
        }
        WarningKind::UnusedConstant => {
            println!("Unused constant '{}'", w.msg);
        }
        WarningKind::UnusedFunctionArgument => {
            println!("Unused function argument '{}'", w.msg);
        }
        WarningKind::UnusedFunctionReturnValue => {
            println!("Unused function return value '{}'", w.msg);
        }
        WarningKind::UnusedFunctionParameter => {
            println!("Unused function parameter '{}'", w.msg);
        }
        WarningKind::UnusedFunctionParameterDefaultValue => {
            println!("Unused function parameter default value '{}'", w.msg);
        }
        WarningKind::PointlessExport => {
            println!("Pointless export '{}'", w.msg);
        }
    }
}
