use colour::yellow_ln_bold;
use std::path::PathBuf;
use crate::compiler::compiler_errors::ErrorLocation;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

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
        location: TextLocation,
        warning_kind: WarningKind,
        file_path: InternedPath,
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
        location: TextLocation,
        warning_kind: WarningKind,
        file_path: PathBuf,
        string_table: &mut StringTable,
    ) -> CompilerWarning {
        let interned_path = InternedPath::from_path_buf(&file_path, string_table);
        CompilerWarning {
            msg: msg.to_owned(),
            location,
            warning_kind,
            file_path: interned_path,
        }
    }

    /// Get the file path as a PathBuf for display purposes
    pub fn file_path_display(&self, string_table: &StringTable) -> PathBuf {
        self.file_path.to_path_buf(string_table)
    }

    /// Get the file path as a string for display purposes
    pub fn file_path_string(&self) -> String {
        format!("{}", self.file_path.to_string())
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
    println!("File: {}", w.file_path_string(string_table));
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
