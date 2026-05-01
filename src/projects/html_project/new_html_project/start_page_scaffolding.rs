//! Scaffold templates for `bean new html`.
//!
//! WHAT: Owns the generated content for `#config.bst`, `src/#page.bst`, manifests, and `.gitignore`.
//! WHY: Centralises template strings so they are not scattered through write logic.

/// Escape a string for use in a Beanstalk `#name = "..."` config literal.
///
/// Minimum escaping: backslash and double-quote.
pub fn escape_beanstalk_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Generate the content for `#config.bst`.
pub fn config_template(project_name: &str) -> String {
    let escaped = escape_beanstalk_string_literal(project_name);
    format!(
        r#"# project = "html"
# entry_root = "src"
# dev_folder = "dev"
# output_folder = "release"
# page_url_style = "trailing_slash"
# redirect_index_html = true
# name = "{escaped}"
# version = "0.1.0"
# author = ""
# license = "MIT"
# html_lang = "en"
"#
    )
}

/// Return the exact starter content for `src/#page.bst`.
pub fn page_template() -> &'static str {
    r#"# page_title = "Welcome"
# page_head = [$html:
    <style>
        [$css:
            body {
                background-color: light-dark(hsl(125, 67%, 97%), hsl(203, 68%, 8%));
                padding: var(--bst-spacing--small);
            }
        ]
    </style>
]

[$markdown:
    # Welcome to Beanstalk

    Here's the @https://nyejames.github.io/beanstalk/docs/ (documentation).

    Use **bean dev** to start the development server and see your changes to this page in real time!
]
"#
}

/// Return the empty HTML build manifest content.
pub fn manifest_template() -> &'static str {
    "# beanstalk-manifest v2\n# builder: html\n# managed_extensions: .html,.js,.wasm\n"
}

/// Return the default `.gitignore` content for a new HTML project.
pub fn gitignore_template() -> &'static str {
    "# Beanstalk development output\n/dev\n\n# OS/editor noise\n.DS_Store\n.vscode/\n.idea/\n"
}

/// Return the block appended to an existing `.gitignore` when it is missing.
pub fn gitignore_append_block() -> &'static str {
    "\n# Beanstalk\n/dev\n"
}
