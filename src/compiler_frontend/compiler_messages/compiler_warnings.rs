use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;

#[derive(Clone, Debug)]
pub struct CompilerWarning {
    pub msg: String,
    pub location: SourceLocation,
    pub warning_kind: WarningKind,
}

impl CompilerWarning {
    pub fn new(msg: &str, location: SourceLocation, warning_kind: WarningKind) -> CompilerWarning {
        CompilerWarning {
            msg: msg.to_owned(),
            location,
            warning_kind,
        }
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
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
    MalformedCssTemplate,
    MalformedHtmlTemplate,
    BstFilePathInTemplateOutput,
    LargeTrackedAsset,
    IdentifierNamingConvention,
    ImportAliasCaseMismatch,
}
