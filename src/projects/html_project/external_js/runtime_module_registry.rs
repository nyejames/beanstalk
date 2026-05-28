//! Builder-owned registry of core JS runtime modules.
//!
//! WHAT: tracks which JS module specifiers are allowed in `import ... from "..."`
//!       statements inside Beanstalk JS library files, and holds their authored source
//!       for later emission by the HTML builder.
//! WHY: the HTML builder owns the set of core runtime modules; the parser only validates
//!      that JS imports match the registered set. Keeping the registry in `external_js`
//!      lets the parser and builder share one definition while staying within the
//!      HTML-project boundary.

/// A single builder-registered core JS runtime module.
///
/// WHAT: describes a module specifier and its authored JS source that the builder
///       will emit as a runtime asset.
/// WHY: keeps the runtime module contract (specifier + source) separate from
///      the registry that collects them, so v1 and later versions share one shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreJsRuntimeModule {
    pub specifier: String,
    pub source: String,
    /// Symbol names exported by this runtime module that are valid import targets.
    pub exported_names: Vec<String>,
}

impl CoreJsRuntimeModule {
    /// Creates a v1 `@beanstalk/runtime` module with `bstOk` and `bstErr` exports.
    ///
    /// WHAT: provides the small plain JS source that fallible JS library functions
    ///       import to return structured success/error wrappers.
    /// WHY: the runtime wrapper contract must match the glue the backend generates.
    pub fn beanstalk_runtime_v1() -> Self {
        Self {
            specifier: "@beanstalk/runtime".to_owned(),
            source: BEANSTALK_RUNTIME_V1_SOURCE.to_owned(),
            exported_names: vec!["bstOk".to_owned(), "bstErr".to_owned()],
        }
    }
}

/// v1 source for `@beanstalk/runtime`.
///
/// `bstOk(value)` produces a success wrapper.
/// `bstOk()` with no argument also succeeds; `value` is `undefined`.
/// `bstErr(code, message)` produces an error wrapper with enough shape for later
/// dev/debug glue validation.
const BEANSTALK_RUNTIME_V1_SOURCE: &str = r#"export function bstOk(value) {
    return { ok: true, value: value };
}

export function bstErr(code, message) {
    return { ok: false, error: { code, message } };
}
"#;

/// Builder-owned registry of allowed core JS runtime module imports.
///
/// WHAT: holds the set of registered core JS runtime modules and their sources.
/// WHY: v1 registers only `@beanstalk/runtime`, but the shape is kept extensible
///      so future phases can register additional core runtime modules without
///      changing the scanner logic.
pub struct RuntimeModuleRegistry {
    modules: Vec<CoreJsRuntimeModule>,
}

impl RuntimeModuleRegistry {
    /// Creates an empty registry with no registered modules for parser/registry tests.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Creates the v1 registry containing only `@beanstalk/runtime`.
    pub fn v1() -> Self {
        Self {
            modules: vec![CoreJsRuntimeModule::beanstalk_runtime_v1()],
        }
    }

    /// Returns true if the given module specifier is registered.
    pub fn is_registered(&self, specifier: &str) -> bool {
        self.modules.iter().any(|m| m.specifier == specifier)
    }

    /// Returns the list of registered runtime modules.
    #[cfg(test)]
    pub fn registered_modules(&self) -> &[CoreJsRuntimeModule] {
        &self.modules
    }

    /// Returns true if the given name is exported by the registered module specifier.
    pub fn is_exported_name(&self, specifier: &str, name: &str) -> bool {
        self.modules
            .iter()
            .find(|m| m.specifier == specifier)
            .is_some_and(|m| m.exported_names.iter().any(|n| n == name))
    }

    /// Returns the JS source for a registered module specifier, if any.
    pub fn module_source(&self, specifier: &str) -> Option<&str> {
        self.modules
            .iter()
            .find(|m| m.specifier == specifier)
            .map(|m| m.source.as_str())
    }
}
