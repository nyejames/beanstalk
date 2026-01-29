use crate::settings::Config;
use fs_extra::dir::{CopyOptions, copy};
use std::{env, fs, path::PathBuf};

pub fn create_project(
    user_project_path: PathBuf,
    project_name: &str,
) -> Result<(), fs_extra::error::Error> {
    // Get the current directory
    let current_dir = env::current_dir()?;

    // Create the full path to the user specified path
    let full_path = current_dir.join(user_project_path);

    // Create a user specified path
    fs::create_dir_all(&full_path)?;

    let options = CopyOptions::new(); // Default options

    // Copy project directory from /html_project_template folder to user specified path
    copy(
        PathBuf::from("build_system/html_project/html_project_template")
            .canonicalize()
            .unwrap_or_else(|_| panic!("Failed to canonicalize html_project_template/index.html path!")),
        &full_path,
        &options,
    )?;

    fs::rename(full_path.join("html_project_template"), project_name)?;

    println!("Project created at: {:?}", &full_path);

    Ok(())
}
