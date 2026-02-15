use crate::build_system::build;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::display_messages::print_compiler_messages;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::settings::BEANSTALK_FILE_EXTENSION;
use crate::settings::Config;
use crate::{Flag, settings};
use saying::say;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{
    fs::{self, metadata},
    io::{BufReader, prelude::*},
    net::{TcpListener, TcpStream},
};

//noinspection HttpUrlsUsage
pub fn start_dev_server(user_entry_path: &str, flags: &[Flag]) {
    // TODO: get a custom port number from a flag specified by the user
    let url = "127.0.0.1:6347";
    let listener = match TcpListener::bind(url) {
        Ok(l) => l,
        Err(e) => {
            say!(Red "Errors while starting dev server: \n");
            say!(Red "{:?}", e);
            return;
        }
    };

    say!(Bold "Dev Server created on: ");
    say!(Green "http://{}", url.replace("127.0.0.1", "localhost"));

    // TODO: Now separately build all the runtime hooks / project structure

    let mut modified = SystemTime::UNIX_EPOCH;

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        match handle_connection(stream, user_entry_path, &mut modified, flags) {
            Ok(warnings) => {
                for warning in warnings {
                    say!(Red #warning);
                }
            }
            Err(messages) => {
                print_compiler_messages(messages);
            }
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    path: &str,
    last_modified: &mut SystemTime,
    flags: &[Flag],
) -> Result<Vec<CompilerWarning>, CompilerMessages> {
    let buf_reader = BufReader::new(&mut stream);

    let mut messages = CompilerMessages::new();

    let config = Config::new(PathBuf::from(path));
    let builder = Box::new(HtmlProjectBuilder::new());

    let dir_404 = &config
        .entry_dir
        .join(&config.dev_folder)
        .join("404")
        .with_extension("html");

    let mut contents = fs::read(dir_404).unwrap_or_default();

    let mut status_line = "HTTP/1.1 404 NOT FOUND";
    let mut content_type = "text/html";

    let request_line = buf_reader.lines().next().unwrap();

    match request_line {
        Ok(request) => {
            // HANDLE REQUESTS
            if request == "GET / HTTP/1.1" {
                let p = match get_home_page_path(false, &config) {
                    Ok(p) => p,
                    Err(e) => {
                        messages.errors.push(e);
                        return Err(messages);
                    }
                };
                contents = match fs::read(&p) {
                    Ok(content) => content,
                    Err(e) => {
                        messages.errors.push(CompilerError::new_file_error(
                            &p,
                            format!("Error reading home page: {:?}", e),
                            HashMap::new(),
                        ));
                        return Err(messages);
                    }
                };
                status_line = "HTTP/1.1 200 OK";

            // This is a request to check if the file has been modified
            } else if request.starts_with("HEAD /check") {
                // the check request has the page url as a query parameter after the /check
                let request_path = request.split("?page=").nth(1);

                let parsed_url = match request_path {
                    Some(p) => {
                        let page_path = p.split_whitespace().collect::<Vec<&str>>()[0];
                        if page_path == "/" {
                            match get_home_page_path(true, &config) {
                                Ok(p) => p,
                                Err(e) => {
                                    messages.errors.push(e);
                                    return Err(messages);
                                }
                            }
                        } else {
                            PathBuf::from(path)
                                .join(&config.src)
                                .join(page_path)
                                .with_extension(BEANSTALK_FILE_EXTENSION)
                        }
                    }
                    None => match get_home_page_path(true, &config) {
                        Ok(p) => p,
                        Err(e) => {
                            messages.errors.push(e);
                            return Err(messages);
                        }
                    },
                };

                let global_file_path = &PathBuf::from(&path)
                    .join(&config.src)
                    .join(settings::GLOBAL_PAGE_KEYWORD)
                    .with_extension(BEANSTALK_FILE_EXTENSION);

                // Get the metadata of the file to check if hot reloading is needed
                // Check if globals have been modified
                let global_file_modified = if metadata(global_file_path).is_ok() {
                    match has_been_modified(&global_file_path, last_modified) {
                        Ok(bool) => bool,
                        Err(e) => {
                            messages.errors.push(e);
                            return Err(messages);
                        }
                    }
                } else {
                    false
                };

                // Check if the file has been modified
                let has_been_modified = match has_been_modified(&parsed_url, last_modified) {
                    Ok(bool) => bool,
                    Err(e) => {
                        messages.errors.push(e);
                        return Err(messages);
                    }
                };

                if has_been_modified || global_file_modified {
                    say!(Blue "Changes detected for ", #parsed_url);
                    let messages = build::build_project(builder, path, &flags);

                    if messages.errors.is_empty() {
                        status_line = "HTTP/1.1 205 Reset Content";
                    } else {
                        return Err(messages);
                    }
                } else {
                    status_line = "HTTP/1.1 200 OK";
                }
            } else if request.starts_with("GET /") {
                // Get a requested path
                let file_path = request.split_whitespace().collect::<Vec<&str>>()[1];

                // Set the Content-Type based on the file extension
                let path_to_file = &PathBuf::from(file_path)
                    .join(&config.dev_folder)
                    .join(file_path.strip_prefix("/").unwrap_or(file_path));

                let file_requested = if file_path.ends_with(".js") {
                    content_type = "application/javascript";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".wasm") {
                    content_type = "application/wasm";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".css") {
                    content_type = "text/css";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".png") {
                    content_type = "image/png";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".jpg") {
                    content_type = "image/jpeg";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".ico") {
                    content_type = "image/ico";
                    fs::read(path_to_file)
                } else if file_path.ends_with(".webmanifest") {
                    content_type = "application/manifest+json";
                    fs::read(path_to_file)
                } else {
                    let page_path = path_to_file.with_extension("html");
                    fs::read(page_path)
                };

                match file_requested {
                    Ok(c) => {
                        // Make sure the path does not try to access any directories outside /dev
                        if !file_path.contains("..") {
                            contents = c;
                            status_line = "HTTP/1.1 200 OK";
                        } else {
                            say!(Red
                                "Dev Server Error: File tried to access outside of /dev directory"
                            );
                            contents = String::new().into_bytes();
                            status_line = "HTTP/1.1 404 NOT FOUND";
                        }
                    }

                    Err(_) => {
                        say!(Red
                            "File not found. Site made a GET request for: ",
                            #path_to_file
                        );
                    }
                }
            }
        }
        _ => {
            say!(Red "Error reading request line");
        }
    }

    let string_response = format!(
        "{}\r\nContent-Length: {}\r\nContent-Type: {}\r\n\r\n",
        status_line,
        contents.len(),
        content_type,
    );

    let response = &[string_response.as_bytes(), &contents].concat();

    match stream.write_all(response) {
        Ok(_) => Ok(messages.warnings),
        Err(e) => {
            messages.errors.push(CompilerError::new_file_error(
                &PathBuf::from(path),
                format!("Error writing response: {:?}", e),
                // TODO: add some metadata to this error
                HashMap::new(),
            ));
            Err(messages)
        }
    }
}

fn has_been_modified(path: &PathBuf, modified: &mut SystemTime) -> Result<bool, CompilerError> {
    // Check if it's a file or directory
    let path_metadata = match metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return Err(CompilerError::new_file_error(
                path,
                format!("Error reading file metadata: {:?}", e),
                // TODO: add some metadata to this error
                HashMap::new(),
            ));
        }
    };

    if path_metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(all) => all,
            Err(e) => {
                return Err(CompilerError::new_file_error(
                    path,
                    format!("Error reading directory: {:?}", e),
                    // TODO: add some metadata to this error
                    HashMap::new(),
                ));
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    return Err(CompilerError::new_file_error(
                        path,
                        format!("Error reading directory entry: {:?}", e),
                        // TODO: add some metadata to this error
                        HashMap::new(),
                    ));
                }
            };

            let meta = match metadata(entry.path()) {
                Ok(m) => m,
                Err(e) => {
                    return Err(CompilerError::new_file_error(
                        path,
                        format!("Error reading directory: {:?}", e),
                        // TODO: add some metadata to this error
                        HashMap::new(),
                    ));
                }
            };

            let modified_time = match meta.modified() {
                Ok(t) => t,
                Err(e) => {
                    return Err(CompilerError::new_file_error(
                        path,
                        format!("Error getting the system time for hot reloading: {:?}", e),
                        // TODO: add some metadata to this error
                        HashMap::new(),
                    ));
                }
            };

            if modified_time > *modified {
                *modified = modified_time;
                return Ok(true);
            }
        }
    }

    if path_metadata.is_file() {
        match path_metadata.modified() {
            Ok(t) => {
                if t > *modified {
                    *modified = t;
                    return Ok(true);
                }
            }
            Err(e) => {
                return Err(CompilerError::new_file_error(
                    path,
                    format!("Error reading the file modification time metadata: {:?}", e),
                    // TODO: add some metadata to this error
                    HashMap::new(),
                ));
            }
        }
    }

    Ok(false)
}

fn get_home_page_path(
    source_folder: bool,
    project_config: &Config,
) -> Result<PathBuf, CompilerError> {
    let root_src_path = if source_folder {
        PathBuf::from(&project_config.entry_dir).join(&project_config.src)
    } else {
        PathBuf::from(&project_config.entry_dir).join(&project_config.dev_folder)
    };

    let src_files = match fs::read_dir(&root_src_path) {
        Ok(m) => m,
        Err(e) => {
            return Err(CompilerError::new_file_error(
                &project_config.entry_dir,
                format!("Error trying to read the source directory path: {:?}", e),
                // TODO: add some metadata to this error
                HashMap::new(),
            ));
        }
    };

    // Look for the first file that starts with '#page' in the src directory
    let mut first_page = None;
    for entry in src_files {
        first_page = match entry {
            Ok(e) => {
                let page = e.path();
                if source_folder {
                    if page
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(settings::COMP_PAGE_KEYWORD)
                    {
                        Some(page)
                    } else {
                        continue;
                    }
                } else if page
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with(settings::INDEX_PAGE_NAME)
                {
                    Some(page)
                } else {
                    continue;
                }
            }

            Err(e) => {
                return Err(CompilerError::new_file_error(
                    &project_config.entry_dir,
                    format!("Error reading the source directory file: {:?}", e),
                    // TODO: add some metadata to this error
                    HashMap::new(),
                ));
            }
        };
    }

    match first_page {
        Some(index_page_path) => Ok(index_page_path),
        None => {
            Err(CompilerError::new_file_error(
                &root_src_path,
                format!(
                    "No page found in {:?} directory",
                    if source_folder {
                        &project_config.src
                    } else {
                        &project_config.dev_folder
                    }
                ),
                // TODO: add some metadata to this error
                HashMap::new(),
            ))
        }
    }
}
