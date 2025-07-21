use crate::compiler::wasm_codegen::wasm_emitter::WasmModule;

// TODO
pub fn create_wasm_bump_allocator(heap_start: i32) -> WasmModule {
    WasmModule::new()
}
