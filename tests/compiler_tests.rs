use std::fs;
use wasmer::{Instance, Module, Store, Value, imports};

#[test]
fn test_all_examples_in_folder() {
    let mut errors = Vec::new();

    for entry in std::fs::read_dir("tests/cases").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("bs") {
            let source = fs::read_to_string(&path).unwrap();

            // TODO: Get some wasm back from the compiler
            let wasm: Vec<u8> = Vec::new();

            // assume each file defines `fn main() -> Int`
            let got = run_wasm_and_get_i32_return(&wasm, "main");
            // derive expected from filename, e.g. "cases/42.bs" → expect 42?
            let expected: i32 = path.file_stem().unwrap().to_str().unwrap().parse().unwrap();
            if got != expected {
                errors.push(format!(
                    "{}: got {}, expected {}",
                    path.display(),
                    got,
                    expected
                ));
            }
        }
    }

    if !errors.is_empty() {
        panic!("Some tests failed:\n{}", errors.join("\n"));
    }
}

fn run_wasm_and_get_i32_return(wasm_bytes: &[u8], func_name: &str) -> i32 {
    // 1) create a Store
    let mut store = Store::default();

    // 2) compile the module
    let module = Module::new(&store, wasm_bytes).expect("Wasm module failed to compile");

    // 3) instantiate with no imports (or whatever you need)
    let import_object = imports! {};
    let instance =
        Instance::new(&mut store, &module, &import_object).expect("Wasm instantiation failed");

    // 4) call the exported function and read an i32
    let func = instance.exports.get_function(func_name).unwrap();
    let results = func.call(&mut store, &[]).unwrap();
    match results[0] {
        Value::I32(n) => n,
        _ => panic!("Expected i32 return"),
    }
}
