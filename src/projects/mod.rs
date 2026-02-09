/// # This module contains all the built-in projects that the compiler ships by default
/// While Beanstalk project builders are intended to be expanded through libraries, 
/// these are core project builders that must always be supported.

// The basic compiler CLI for interacting with the compiler's built-in projects
pub(crate) mod cli;

pub(crate) mod html_project {
    pub(crate) mod dev_server;
    pub(crate) mod html_project_builder;
    pub(crate) mod new_html_project;
}
