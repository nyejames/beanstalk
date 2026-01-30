use fs_extra::dir::{CopyOptions, copy};
use std::{env, fs, path::PathBuf};
use colour::red_ln;

pub fn create_project(
    user_project_path: PathBuf,
    project_name: &str,
) -> Result<(), fs_extra::error::Error> {
    // Get the current directory
    let current_dir = env::current_dir()?;

    // TODO: CHANGE ALL THIS TO ACTUALLY CREATE THE FILES AND DIRECTORIES
    // Copying will be weird when distributing the compiler

    // Create the full path to the user specified path
    let full_path = current_dir.join(user_project_path);

    // Create a user-specified path
    fs::create_dir_all(&full_path)?;

    let options = CopyOptions::new(); // Default options

    let os_safe_path: PathBuf = full_path
        .join("build_system")
        .join("html_project")
        .join("html_project_template");

    red_ln!("OS safe path: {:?}", os_safe_path);

    // Copy project directory from /html_project_template folder to user specified path
    copy(
        os_safe_path,
        &full_path,
        &options,
    )?;

    fs::rename(full_path.join("html_project_template"), project_name)?;

    println!("Project created at: {:?}", &full_path);

    Ok(())
}
