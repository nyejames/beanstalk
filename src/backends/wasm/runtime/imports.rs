//! Host import identifiers reserved by the Wasm backend.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WasmHostFunction {
    /// Generic host log output used by `io`-style calls in the current backend.
    LogString,
}

impl WasmHostFunction {
    pub(crate) fn module_name(self) -> &'static str {
        "host"
    }

    pub(crate) fn item_name(self) -> &'static str {
        match self {
            WasmHostFunction::LogString => "log_string",
        }
    }
}
