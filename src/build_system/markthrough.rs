// Markthrough are files that are equivalent to Beanstalk's template syntax apart from a few key things:
// - They can have an optional frontmatter. the body of a template
// - They do not generate Wasm, just HTML and JS bindings
// - They automatically inherit the Beanstalk Markdown Page style,
// so every template uses a Markdown formatter in its body and inherits all the HTML styles from it
