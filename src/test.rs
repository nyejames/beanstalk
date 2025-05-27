use crate::Error;
use crate::dev_server;
use colour::{green_ln_bold, yellow_ln_bold};
use std::path::Path;

pub fn test_build(entry_path: &Path, output_info_level: i32) -> Result<(), Error> {
    // TODO - Compiler tests
    // Eventually this should perform a test suit for the compiler
    // Atm, it just automatically sets the output info level to highest
    // So loads of stuff gets printed to the console so see the output of each stage of the compiler

    // Read content from a test file
    yellow_ln_bold!("\nTESTING FILE\n");

    if entry_path.is_dir() {
        dev_server::start_dev_server(entry_path, output_info_level)?;
    }

    green_ln_bold!("Test complete!");
    Ok(())
}
