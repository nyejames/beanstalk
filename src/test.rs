use crate::Error;
use crate::dev_server;
use colour::{green_ln_bold, yellow_ln_bold};
use std::path::PathBuf;

pub fn test_build(entry_path: &PathBuf) -> Result<(), Error> {
    // TODO - Compiler tests
    // Eventually this should perform a test suit for the compiler
    // Atm, it just automatically sets the output info level to highest
    // So loads of stuff gets printed to the console so see the output of each stage of the compiler

    // CBA to make the full CLI for this atm,
    // so this number will be edited depending on what part if the compiler is being worked on
    let output_info_level = 10;

    // Read content from a test file
    yellow_ln_bold!("\nTESTING FILE\n");

    if entry_path.is_dir() {
        dev_server::start_dev_server(entry_path, output_info_level)?;
    }

    green_ln_bold!("Test complete!");
    Ok(())
}
