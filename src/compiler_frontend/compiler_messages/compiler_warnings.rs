use crate::compiler_frontend::source_location::SourceLocation;

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
}
