# Package-Scoped External Symbol Registration Plan

## Goal

Refactor external platform package registration so external functions and types are identified by package-scoped symbols, not globally unique symbol names.

This prevents collisions like:

```text
@web/canvas/open
@app/fs/open
@std/io/open
and prepares the registry for larger platform APIs such as canvas, DOM, storage, audio, files, and Rust/Wasm host bindings.
Current problem
The external package registry currently stores functions and types by stable IDs, but package import resolution still relies on global name maps such as:
name_to_function_id
name_to_type_id
This means two packages cannot safely expose the same symbol name.
That is acceptable for the current tiny builtin set, but it is not correct for general platform packages.
Desired model
External symbols are registered and resolved by full package scope:
(package_path, symbol_name) -> ExternalSymbolId
or conceptually:
@std/io/io        -> ExternalFunctionId
@std/io/IO        -> ExternalTypeId
@web/canvas/open  -> ExternalFunctionId
@app/fs/open      -> ExternalFunctionId
Bare-name lookup should only happen after import/prelude visibility has selected a specific external symbol.

Invariants
External packages are not source files.
External source visibility is separate from source declaration visibility.
visible_symbol_paths is only for source declarations and compiler-owned builtin declarations.
visible_external_symbols is for imported/prelude external functions and types.
No user-authored external call should resolve through global bare-name registry lookup.
Package-local duplicate names are errors.
Same symbol names across different packages are allowed.
HIR stores stable external IDs, not names or import paths.
Backend lowering maps external IDs to backend-specific runtime/import keys.
Style directives remain separate from external packages.

Phase 1: Add package-scoped symbol identity
Add package-local symbol key
In src/compiler_frontend/external_packages.rs, add:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalPackageId(pub u32);
Then add a package-scoped key:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalPackageSymbolKey {
    pub package_id: ExternalPackageId,
    pub symbol_name: StringId,
}
If StringId is not practical inside registry metadata yet because package definitions still use &'static str, use an interim private key:
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExternalPackageSymbolKey {
    package_path: &'static str,
    symbol_name: &'static str,
}
Prefer StringId later when registry construction has access to the shared StringTable.
Add symbol path concept
Add:
pub enum ExternalSymbolId {
    Function(ExternalFunctionId),
    Type(ExternalTypeId),
}
Keep this as the public visibility identity.

Phase 2: Replace global name maps
Replace or deprecate:
name_to_function_id
name_to_type_id
with:
function_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalFunctionId>
type_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalTypeId>
Keep only a narrow prelude map if needed:
prelude_symbols_by_name: HashMap<StringId, ExternalSymbolId>
or, while still using static strings:
prelude_symbols_by_name: HashMap<&'static str, ExternalSymbolId>
Rule
Global bare-name maps are only allowed for prelude resolution.
General package import resolution must use:
resolve_package_function(package_path, symbol_name)
resolve_package_type(package_path, symbol_name)
and those functions must not internally consult a global name-to-ID map.

Phase 3: Centralize registration
Add registration helpers so each external function/type is defined once.
impl ExternalPackageRegistry {
    fn register_package(&mut self, package: ExternalPackage) -> Result<ExternalPackageId, CompilerError>;

    fn register_function(
        &mut self,
        package_path: &'static str,
        id: ExternalFunctionId,
        function: ExternalFunctionDef,
    ) -> Result<(), CompilerError>;

    fn register_type(
        &mut self,
        package_path: &'static str,
        id: ExternalTypeId,
        type_def: ExternalTypeDef,
    ) -> Result<(), CompilerError>;
}
Each helper must update every relevant index:
packages
functions_by_id
types_by_id
function_ids_by_package_symbol
type_ids_by_package_symbol
method index, if applicable
prelude map, if explicitly requested
Do not manually insert the same function/type into several maps at call sites.
Duplicate checks
Reject duplicates within the same package:
@web/canvas/open
@web/canvas/open
Allow duplicates across packages:
@web/canvas/open
@app/fs/open
Reject package path duplicates unless intentionally extending an existing package through one controlled helper.

Phase 4: Update import binding resolution
In src/compiler_frontend/ast/import_bindings.rs, update virtual package import resolution so this path:
import @web/canvas/open
does:
1. Find longest matching package prefix: @web/canvas
2. Extract symbol name: open
3. Resolve (@web/canvas, open) through package-scoped registry indexes
4. Insert ExternalSymbolId into visible_external_symbols under source name `open`
Do not call global resolve_function("open") or resolve_type("open").
Grouped import behavior
For:
import @web/canvas {Canvas, open}
each expanded import should resolve against @web/canvas.
Collision behavior
Preserve file-local collision checks:
source declaration/import named open conflicts with explicit external import open;
explicit external import open conflicts with another visible external symbol named open;
prelude external symbols do not override source declarations or explicit imports.

Phase 5: Update prelude registration
Prelude symbols should be explicit registry metadata, not inferred through global name lookup.
Add a helper:
fn register_prelude_symbol(
    &mut self,
    public_name: &'static str,
    symbol_id: ExternalSymbolId,
) -> Result<(), CompilerError>;
Then register:
io -> ExternalFunctionId::Io
IO -> ExternalTypeId(0)
Import binding should populate visible_external_symbols from the prelude map.
Rule
Prelude lookup may be bare-name because the prelude is already an explicitly chosen default import set.
No other external symbol lookup should be bare-name global.

Phase 6: Update parser and type resolution call sites
Search for all usages of:
resolve_function(...)
get_function(...)
resolve_type(...)
get_type(...)
name_to_function_id
name_to_type_id
Classify each usage:
Allowed
backend/internal lookup by ExternalFunctionId;
prelude setup;
test-only registry helpers;
diagnostics that already have a resolved ID.
Not allowed
expression parsing of user-authored identifiers;
type resolution of user-authored annotations;
import resolution that has package context available.
User-facing parsing and type resolution must go through:
ScopeContext::lookup_visible_external_function(...)
ScopeContext::lookup_visible_external_type(...)
or through package-scoped import binding.

Phase 7: Update tests
Add or update integration tests.
Required tests
Same symbol name in different packages is allowed
Use test packages if production packages do not yet expose duplicate names.
Expected:
@pkg/a/open
@pkg/b/open
can both exist in the registry.
Import selects correct package symbol
import @pkg/a/open

open(...)
must resolve to @pkg/a/open, not @pkg/b/open.
Duplicate visible names conflict
import @pkg/a/open
import @pkg/b/open
should fail unless aliasing is later introduced.
Prelude does not override source symbol
If a source file declares or imports io, prelude io should not silently replace it.
Non-imported external still fails
__bs_collection_length({})
should fail unless explicitly imported or prelude-visible.
Unknown package symbol still gives package-specific diagnostic
import @std/io/missing
should report missing symbol in package @std/io.

Phase 8: Update docs and comments
Update docs/compiler-design-overview.md.
External platform packages section
Add:
External symbols are registered by package scope. The same symbol name may exist in multiple packages. Import binding resolves a concrete `(package, symbol)` pair into an `ExternalSymbolId`, then stores that ID in `visible_external_symbols`.
Add:
Global bare-name external lookup is only valid for the builder prelude. User-authored external calls and type annotations must resolve through file-local external visibility.
Stage 4 AST section
Clarify:
`visible_external_symbols` stores source-visible names mapped to already-resolved external IDs. Later expression and type resolution never re-resolves those names globally.
Code comments
Update comments in:
src/compiler_frontend/external_packages.rs
src/compiler_frontend/ast/import_bindings.rs
src/compiler_frontend/ast/module_ast/scope_context.rs
Comments should explain:
package-scoped registration;
prelude as the only bare-name external lookup;
external visibility as resolved IDs, not deferred package lookups.
Remove or rewrite comments implying external symbols are globally unique.

Phase 9: Review touched areas
Before final validation, review all touched areas:
src/compiler_frontend/external_packages.rs
src/compiler_frontend/ast/import_bindings.rs
src/compiler_frontend/ast/module_ast/scope_context.rs
src/compiler_frontend/ast/expressions/function_calls.rs
src/compiler_frontend/ast/expressions/parse_expression_identifiers.rs
src/compiler_frontend/ast/statements/body_symbol.rs
src/compiler_frontend/ast/type_resolution.rs
src/compiler_frontend/declaration_syntax/type_syntax.rs
src/backends/js/js_host_functions.rs
src/backends/wasm/hir_to_lir/imports.rs
docs/compiler-design-overview.md
Check specifically:
no user-authored external symbol path goes through global bare-name lookup;
import binding resolves package symbols into stable IDs once;
parser uses visible external IDs only;
diagnostics still show source-level names;
backend lowering still receives ExternalFunctionId;
no new user-input panic paths;
no compatibility wrapper preserves the old global-name behavior.

Phase 10: Validation
Run:
cargo fmt --check
cargo clippy
cargo test
cargo run tests
Then run the broader validation command if available:
just validate
If formatting is required, run:
cargo fmt
then repeat the validation checks.
Acceptance criteria
External package symbols are registered package-scope, not global-name scope.
Two packages can define the same symbol name without registry collision.
Explicit imports select the correct package symbol.
Duplicate visible names still error clearly.
Prelude still works for io and IO.
Non-prelude external symbols are not visible without import.
HIR/backend lowering still uses ExternalFunctionId.
Docs describe package-scoped registration and the prelude exception.