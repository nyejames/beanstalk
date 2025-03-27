pub fn add_wasm_fn(wasm_module: &mut wasm_encoder::Module, name: &str, func: wasm_encoder::Func) {
    // Encode the type section for this function
    let params: Vec<ValType> = args.iter()
        .map(|arg| arg.to_wasm_type())
        .flatten().collect();

    let results = return_type.iter()
        .map(|arg| arg.to_wasm_type())
        .flatten().collect();

    types.ty().function(params, results);

    wasm_module.section(&types);

    // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();

    // Create the function section
    let mut functions = FunctionSection::new();
    functions.function(type_index);
    wasm_module.section(&functions);

    // Encode the export section.
    wasm_export_section.export(&name, ExportKind::Func, type_index);
    wasm_module.section(&wasm_export_section);
    type_index += 1;

    js.push_str(&func);
    wat.push_str(&func_body.wat);
    wat_global_initialisation.push_str(&func_body.wat_globals);
}