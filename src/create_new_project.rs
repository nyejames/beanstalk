use crate::settings::Config;
use fs_extra::dir::{CopyOptions, copy};
use std::{env, fs, path::PathBuf};

pub fn create_project(
    user_project_path: PathBuf,
    project_name: &String,
) -> Result<(), fs_extra::error::Error> {
    // Get the current directory
    let current_dir = env::current_dir()?;

    // Create the full path to the user specified path
    let full_path = current_dir.join(user_project_path);

    // Create user specified path
    fs::create_dir_all(&full_path)?;

    let options = CopyOptions::new(); // Default options

    // Copy project directory from /html_project_template folder to user specified path
    copy(
        PathBuf::from("src/html_project_template"),
        &full_path,
        &options,
    )?;

    // Create new dev folder
    let dev_folder_name = Config::default().dev_folder;
    let release_folder_name = Config::default().release_folder;
    let new_dev_folder = &full_path
        .join("html_project_template")
        .join(dev_folder_name);
    // Copy the dist folder to the dev folder
    copy(
        full_path
            .join("html_project_template")
            .join(release_folder_name),
        new_dev_folder,
        &options.content_only(true),
    )?;

    fs::rename(full_path.join("html_project_template"), project_name)?;

    println!("Project created at: {:?}", &full_path);

    Ok(())
}
