//! Host import identifiers reserved by the Wasm backend.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WasmHostFunction {}

impl WasmHostFunction {
    pub(crate) fn module_name(self) -> &'static str {
        match self {}
    }

    pub(crate) fn item_name(self) -> &'static str {
        match self {}
    }
}
