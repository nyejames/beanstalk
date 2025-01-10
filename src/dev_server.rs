use crate::settings::Config;
use crate::{build, settings, CompileError, Error, ErrorType};
use colour::{blue_ln, dark_cyan_ln, green_ln_bold, grey_ln, print_bold, red_ln};
use std::path::PathBuf;
use std::time::SystemTime;
use std::{
    fs::{self, metadata},
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
    time::Instant,
};

//noinspection HttpUrlsUsage
pub fn start_dev_server(path: &PathBuf, project_config: &mut Config) -> Result<(), Error> {
    let url = "127.0.0.1:6969";
    let listener = match TcpListener::bind(url) {
        Ok(l) => l,
        Err(e) => {
            return Err(Error {
                msg: format!("TCP error: {e}. Is there an instance of this local host server already running on {url}?"),
                line_number: 0,
                file_path: PathBuf::new(),
                error_type: ErrorType::DevServer
            })
        }
    };

    let path = get_current_dir()?.join(path.to_owned());

    build_project(&path, false, project_config)?;

    let mut modified = SystemTime::UNIX_EPOCH;

    print_bold!("Dev Server created on: ");
    green_ln_bold!("http://{}", url.replace("127.0.0.1", "localhost"));

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        handle_connection(stream, path.clone(), &mut modified, project_config)?;
    }

    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    path: PathBuf,
    last_modified: &mut SystemTime,
    project_config: &mut Config,
) -> Result<(), Error> {
    let buf_reader = BufReader::new(&mut stream);

    // println!("{}", format!("{}/{}/dev/any file should be here", entry_path, path));
    let dir_404 = path.join("dev/404.html");
    let mut contents = match fs::read(&dir_404) {
        Ok(content) => content,
        Err(e) => {
            red_ln!(
                "Error reading 404 file (is there a 404 file in the {:?} directory?): {:?}",
                dir_404,
                e
            );
            vec![]
        }
    };

    let mut status_line = "HTTP/1.1 404 NOT FOUND";
    let mut content_type = "text/html";

    let request_line = buf_reader.lines().next().unwrap();
    match request_line {
        Ok(request) => {
            // HANDLE REQUESTS
            if request == "GET / HTTP/1.1" {
                let p = get_home_page_path(&path, false, &project_config)?;
                contents = match fs::read(&p) {
                    Ok(content) => content,
                    Err(e) => {
                        return Err(Error {
                            msg: format!("Error reading home page: {:?}", e),
                            line_number: 0,
                            file_path: p,
                            error_type: ErrorType::File,
                        });
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
                            get_home_page_path(&path, true, &project_config)?
                        } else {
                            PathBuf::from(&path)
                                .join(&project_config.src)
                                .join(page_path)
                                .with_extension("bs")
                        }
                    }
                    None => get_home_page_path(&path, true, &project_config)?,
                };

                let global_file_path = PathBuf::from(&path)
                    .join(&project_config.src)
                    .join(settings::GLOBAL_PAGE_KEYWORD)
                    .with_extension("bs");

                // Get the metadata of the file to check if hot reloading is needed
                // Check if globals have been modified
                let global_file_modified = if metadata(&global_file_path).is_ok() {
                    has_been_modified(&global_file_path, last_modified)
                } else {
                    false
                };

                // Check if the file has been modified
                if has_been_modified(&parsed_url, last_modified) || global_file_modified {
                    blue_ln!("Changes detected for {:?}", parsed_url);
                    build_project(&path, false, project_config)?;
                    status_line = "HTTP/1.1 205 Reset Content";
                } else {
                    status_line = "HTTP/1.1 200 OK";
                }
            } else if request.starts_with("GET /") {
                // Get requested path
                let file_path = request.split_whitespace().collect::<Vec<&str>>()[1];

                // Set the Content-Type based on the file extension

                let path_to_file = path.join(format!("dev{file_path}"));
                let file_requested = if file_path.ends_with(".js") {
                    content_type = "application/javascript";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".wasm") {
                    content_type = "application/wasm";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".css") {
                    content_type = "text/css";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".png") {
                    content_type = "image/png";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".jpg") {
                    content_type = "image/jpeg";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".ico") {
                    content_type = "image/ico";
                    fs::read(&path_to_file)
                } else if file_path.ends_with(".webmanifest") {
                    content_type = "application/manifest+json";
                    fs::read(&path_to_file)
                } else {
                    let page_path = &path_to_file.with_extension("html");
                    fs::read_to_string(page_path).map(|c| c.into_bytes())
                };

                match file_requested {
                    Ok(c) => {
                        // Make sure the path does not try to access any directories outside /dev
                        if !file_path.contains("..") {
                            contents = c;
                            status_line = "HTTP/1.1 200 OK";
                        } else {
                            red_ln!(
                                "Dev Server Error: File tried to access outside of /dev directory"
                            );
                            contents = String::new().into_bytes();
                            status_line = "HTTP/1.1 404 NOT FOUND";
                        }
                    }

                    Err(_) => {
                        red_ln!("File not found: {:?}", path_to_file);
                    }
                }
            }
        }
        _ => {
            red_ln!("Error reading request line");
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
        Ok(_) => Ok(()),
        Err(e) => Err(Error {
            msg: format!("Error sending response: {:?}", e),
            line_number: 0,
            file_path: PathBuf::from(""),
            error_type: ErrorType::DevServer,
        }),
    }
}

fn build_project(
    build_path: &PathBuf,
    release: bool,
    project_config: &mut Config,
) -> Result<(), Error> {
    dark_cyan_ln!("Building project...");
    let start = Instant::now();

    // TODO - send config file to dev server function and pass it in here
    build::build(build_path, release, project_config)?;

    let duration = start.elapsed();
    grey_ln!("------------------------------------");
    print!("\nProject built in: ");
    green_ln_bold!("{:?}", duration);

    Ok(())
}

fn has_been_modified(path: &PathBuf, modified: &mut SystemTime) -> bool {
    // Check if it's a file or directory
    let path_metadata = match metadata(path) {
        Ok(m) => m,
        Err(_) => {
            red_ln!(
                "Error reading directory (probably doesn't exist): {:?}",
                path
            );
            return false;
        }
    };

    if path_metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(all) => all,
            Err(_) => {
                red_ln!("Error reading directory: {:?}", path);
                return false;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    red_ln!("Error reading entry");
                    return false;
                }
            };

            let meta = match metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => {
                    red_ln!("Error reading file modified metadata");
                    return false;
                }
            };

            let modified_time = match meta.modified() {
                Ok(t) => t,
                Err(_) => {
                    red_ln!("Error reading file modified time in it's metadata");
                    *modified
                }
            };

            if modified_time > *modified {
                *modified = modified_time;
                return true;
            }
        }
    }

    if path_metadata.is_file() {
        match path_metadata.modified() {
            Ok(t) => {
                if t > *modified {
                    *modified = t;
                    return true;
                }
            }
            Err(_) => {
                red_ln!("Error reading file modified time in it's metadata");
                return false;
            }
        }
    }

    false
}

fn get_home_page_path(
    path: &PathBuf,
    src: bool,
    project_config: &Config,
) -> Result<PathBuf, Error> {
    let root_src_path = if src {
        PathBuf::from(&path).join(&project_config.src)
    } else {
        PathBuf::from(&path).join(&project_config.dev_folder)
    };

    let src_files = match fs::read_dir(&root_src_path) {
        Ok(m) => m,
        Err(e) => {
            return Err(Error {
                msg: format!("Error reading root src directory metadata: {:?}", e),
                line_number: 0,
                file_path: root_src_path,
                error_type: ErrorType::File,
            });
        }
    };

    // Look for first file that starts with '#page' in the src directory
    let mut first_page = None;
    for entry in src_files {
        first_page = match entry {
            Ok(e) => {
                let page = e.path();
                if src {
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
                } else {
                    if page
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(settings::INDEX_PAGE_KEYWORD)
                    {
                        Some(page)
                    } else {
                        continue;
                    }
                }
            }
            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading src directory: {:?}", e),
                    line_number: 0,
                    file_path: root_src_path,
                    error_type: ErrorType::File,
                });
            }
        };
    }

    match first_page {
        Some(index_page_path) => Ok(index_page_path),
        None => Err(Error {
            msg: format!(
                "No page found in {} directory: {:?}",
                if src { "src" } else { "dev" },
                first_page
            ),
            line_number: 0,
            file_path: root_src_path,
            error_type: ErrorType::File,
        }),
    }
}

fn get_current_dir() -> Result<PathBuf, Error> {
    match std::env::current_dir() {
        Ok(dir) => Ok(dir),
        Err(e) => Err(Error {
            msg: format!("Error getting current directory: {:?}", e),
            line_number: 0,
            file_path: PathBuf::from(""),
            error_type: ErrorType::DevServer,
        }),
    }
}
