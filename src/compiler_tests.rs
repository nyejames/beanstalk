use std::collections::HashMap;
use std::fs;
use std::path::Path;
use colour::{e_red_ln, e_red_ln_bold, green_ln_bold, print_ln_bold};
use wasmer::{Instance, Module, Store, Value, imports};

#[derive(Clone)]
struct Tests {
    name: String,
    number_of_tests: usize,
    test_results: Vec<i32>,
}
impl Tests {
    pub fn new(name: &str, number_of_tests: usize, test_results: Vec<i32>) -> Tests {
        Tests {
            name: name.to_string(),
            number_of_tests,
            test_results,
        }
    }
}

// This is just for pre-allocating Vecs so won't break anything if it's wrong
// Keeping this correct is purely for optimisation
const NUMBER_OF_TESTS: usize = 6;

pub fn run_all_test_cases() {
    let mut tests_failed: Vec<String> = Vec::new();
    let mut tests_passed: Vec<String> = Vec::with_capacity(NUMBER_OF_TESTS);

    // For each file in the test cases' folder.
    // Iterate through, get the expected test results and check them against the results.
    let read_dir = match fs::read_dir("tests/cases") {
        Ok(e) => e,
        Err(e) => panic!("Could not find or read test/cases dir: {e}"),
    };

    print_ln_bold!("\n----------------------");
    print_ln_bold!("Running Compiler tests");
    print_ln_bold!("----------------------\n");

    let mut tests_ran = 0;
    for file in read_dir {
        let path = match file {
            Ok(e) => e.path(),
            Err(e) => {
                tests_failed.push(format!("Could not read file: {e}"));
                continue
            },
        };
        
        if path.extension().and_then(|s| s.to_str()) == Some("bs") {
            let source = fs::read_to_string(&path).unwrap();

            // TODO: Get some wasm back from the compiler
            let wasm: Vec<u8> = Vec::new();

            tests_ran += 1;

            // Assume each file defines `fn test1() -> Int`
            // But can have multiple test functions test2, test3, etc...
            // Each one will be called and matched against its expected result.
             match get_expected_test_result(&path) {
                Ok(expected_results) => {
                    let results = run_test(&wasm, expected_results);
                    tests_failed.extend(results.fails);
                    tests_passed.extend(results.passes);
                },
                Err(e) => {
                    tests_failed.push(e);
                }
            };
        }
    };

    if tests_failed.is_empty() {
        green_ln_bold!("(■_■¬ ) All {tests_ran} tests passed!");
    }

    if !tests_failed.is_empty() {
        e_red_ln_bold!("{} Tests failed. ({} / {tests_ran} tests passed).\n\n", tests_failed.len(), tests_passed.len());
        for (i, test) in tests_failed.iter().enumerate() {
            e_red_ln!("{i}) {test}");
        }
    }
}

struct TestResult {
    passes: Vec<String>,
    fails: Vec<String>,
}
fn run_test(wasm_bytes: &[u8], tests: Tests) -> TestResult {
    let mut results = TestResult {
        passes: Vec::with_capacity(tests.number_of_tests),
        fails: Vec::new(),
    };

    // 1) create a Store
    let mut store = Store::default();

    // 2) compile the module
    let module = match Module::new(&store, wasm_bytes) {
        Err(e) => {
            results.fails.push(format!("Wasm module failed to compile: {e}"));
            return results
        },
        Ok(m) => m,
    };

    // 3) instantiate with no imports (or whatever you need)
    let import_object = imports! {};

    let instance =
        match Instance::new(&mut store, &module, &import_object) {
            Err(e) => {
                results.fails.push(format!("Wasm module failed to compile: {e}"));
                return results
            }
            Ok(i) => i,
        };

    // 4) call all expected test functions and collect the results
    for test_number in 0..tests.number_of_tests {
        let func = match instance.exports.get_function(&format!("test{}", test_number + 1)) {
            Ok(f) => f,
            Err(e) => {
                results.fails.push(format!("Couldn't find test function: {e}"));
                continue
            },
        };

        let function_returns = match func.call(&mut store, &[]) {
            Ok(r) => r,
            Err(e) => {
                results.fails.push(format!("Test function failed: {e}"));
                continue
            },
        };

        match function_returns[0] {
            Value::I32(n) => {
                let valid_result = tests.test_results[test_number];
                if n == tests.test_results[test_number] {
                    results.passes.push(format!("{}: Test {test_number} passed", tests.name));
                } else {
                    results.fails.push(format!("{}: Test {test_number} Failed. Returned: {n} instead of {valid_result}", tests.name))
                }
            },
            _ => results.fails.push(format!("Test function {} inside {} did not return an i32", test_number + 1, tests.name)),
        };
    }

    results
}


fn get_expected_test_result(filename: &Path) -> Result<Tests, String> {
    let all_tests: HashMap<&str, Tests> = HashMap::from(
        [
            ("basic_math", Tests::new("Basic Maths", 6, vec![7, 6, 5, 4, 3, 2])),
            ("if_statements", Tests::new("Basic If Statements", 1, vec![1])),
        ]
    );

    match filename.file_stem() {
        Some(stem) => {
            let stem = stem.to_str().unwrap();
            match all_tests.get(stem) {
                Some(n) => Ok(n.to_owned()),
                None => Err(format!("Couldn't find an expected output for test: {stem}")),
            }
        }
        None => Err(String::from("No file stem found for this test")),
    }
}


