use crate::Flag;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::settings::Config;
use std::{env, fs};

pub fn create_html_project_template(
    user_project_path: String,
    project_name: &str,
    flags: Vec<Flag>,
) -> Result<(), String> {
    // Get the current directory
    let current_dir = env::current_dir().map_err(|e| e.to_string())?;

    let valid_path = match check_if_valid_path(&user_project_path) {
        Ok(path) => path,
        Err(e) => return Err(e.msg),
    };

    // If the project name is empty, then make a default name
    let name = if project_name.is_empty() {
        "Beanstalk Project"
    } else {
        project_name
    };

    // Create the full path to the user specified path
    let full_path = current_dir.join(valid_path).join(project_name);

    // Create a user-specified path
    fs::create_dir_all(&full_path).map_err(|e| e.to_string())?;

    // Create the default config file
    let mut config = Config::new(full_path.clone());

    config.project_name = String::from(name);
    let config_content = format!(
        "#project_name {name}\n
         #src \"src\"\n
         #output_folder \"dist\"\n
         #name \"html_project\"\n
         #version \"0.1.0\"\n
         #author \"\"\n
         #license \"MIT\"\n
         #libraries {{
             @(core/html),
         }}\n
        "
    );
    fs::write(full_path.join("#config.bst"), config_content).map_err(|e| e.to_string())?;

    // Basic directories
    fs::create_dir(full_path.join("../..")).map_err(|e| e.to_string())?;
    fs::create_dir(full_path.join("release")).map_err(|e| e.to_string())?;
    fs::create_dir(full_path.join("dev")).map_err(|e| e.to_string())?;

    println!("Project created at: {:?}", &full_path);

    Ok(())
}

// pub fn create_default_project_name(full_path: &Path) -> &str {
//     // If any of these stages fail, just set an empty string
//     if full_path.is_file() {
//         // Get the name of the directory this file is in
//         if let Some(last) = full_path.ancestors().last() {
//             last.to_str().unwrap_or_default()
//         } else {
//             ""
//         }
//     } else {
//         // Get the name of this directory
//         full_path.to_str().unwrap_or_default()
//     }
// }
