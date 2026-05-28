//! Warning and rendered-path sinks for AST scope contexts.

use super::*;

impl ScopeContext {
    // --------------------------
    //  Warning / path tracking
    // --------------------------

    pub fn add_var(&mut self, declaration: Declaration) {
        if let Some(visible_declarations) = self.visible_declaration_ids.as_mut() {
            visible_declarations.insert(declaration.id.clone());
        }
        if let Some(name) = declaration.id.name() {
            let index = self.local_declarations.len() as u32;
            self.local_declarations_by_name
                .entry(name)
                .or_default()
                .push(index);
        }
        self.local_declarations.push(declaration);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
    }

    pub fn emit_warning(&self, warning: CompilerDiagnostic) {
        self.shared.emitted_warnings.borrow_mut().push(warning);
    }

    pub fn take_emitted_warnings(&self) -> Vec<CompilerDiagnostic> {
        std::mem::take(&mut *self.shared.emitted_warnings.borrow_mut())
    }

    pub fn record_rendered_path_usages(&self, usages: Vec<RenderedPathUsage>) {
        self.rendered_path_usages.borrow_mut().extend(usages);
    }

    #[cfg(test)]
    pub fn take_rendered_path_usages(&self) -> Vec<RenderedPathUsage> {
        std::mem::take(&mut *self.rendered_path_usages.borrow_mut())
    }
}
