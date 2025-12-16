use crate::compiler::compiler_errors::ErrorLocation;
use crate::compiler::string_interning::StringTable;
use colour::yellow_ln_bold;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CompilerWarning {
    pub msg: String,
    pub location: ErrorLocation,
    pub warning_kind: WarningKind,
    pub file_path: PathBuf,
}

impl CompilerWarning {
    pub fn new(
        msg: &str,
        location: ErrorLocation,
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

    /// Create a CompilerWarning from a PathBuf (for compatibility)
    pub fn new_from_path_buf(
        msg: &str,
        location: ErrorLocation,
        warning_kind: WarningKind,
        file_path: PathBuf,
        string_table: &mut StringTable,
    ) -> CompilerWarning {
        CompilerWarning {
            msg: msg.to_owned(),
            location,
            warning_kind,
            file_path,
        }
    }

    /// Get the file path as a string for display purposes
    pub fn file_path_string(&self) -> String {
        self.file_path.to_string_lossy().to_string()
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
    println!("File: {}", w.file_path_string());
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
