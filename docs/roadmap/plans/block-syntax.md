# Plan: Reserve Keyword-Gated Statement Blocks

## Goal

Add reserved, keyword-only statement block syntax to Beanstalk without implementing future `checked` or `async` semantics yet.

The supported long-term shape should be:

```beanstalk
block:
    value = 1
;

checked:
    -- reserved / feature-gated for future advanced validation
;

async:
    -- reserved / feature-gated for future async lowering
;
```

A bare label-style block must **not** be supported:

```beanstalk
my_label:
    value = 1
;
```

That syntax was previously considered, but should now be removed from docs, plans, diagnostics, and parser expectations. Statement blocks must start with a known reserved block keyword.

## Repo state found

Inspected on `main`:

- `src/compiler_frontend/tokenizer/tokens.rs`
  - `TokenKind` currently includes control-flow keywords such as `If`, `Else`, `Loop`, `Return`, `Break`, `Continue`, `Yield`.
  - There is no current `Block`, `Checked`, or `Async` token.
- `src/compiler_frontend/tokenizer/lexer.rs`
  - Keyword recognition is centralized in `keyword_or_variable()`.
  - `async`, `checked`, and `block` currently fall through as normal `Symbol(...)` identifiers.
  - `$...` syntax is explicitly limited to template heads, so statement-level `$checked:` should not be introduced.
- `src/compiler_frontend/symbols/identifier_policy.rs`
  - `RESERVED_KEYWORD_SHADOWS` mirrors tokenizer keyword policy.
  - It must be updated when new language keywords are added.
- `src/compiler_frontend/ast/statements/body_dispatch.rs`
  - Statement-position parsing is centralized here.
  - Existing statement starters include symbols, `loop`, `if`, `return`, `break`, `continue`, templates, and expression-statement candidates.
  - This is the right place to route `block`, `checked`, and `async`.
- `src/compiler_frontend/ast/statements/body_symbol.rs`
  - Symbol-led statements are dense and handle declaration/call/reference/mutation ambiguity.
  - New block keywords should **not** enter this path.
- `src/compiler_frontend/declaration_syntax/type_syntax.rs`
  - There is already a `TokenKind::Colon` branch in declaration-target type parsing that emits:
    - `Labeled scopes are deferred for Alpha.`
  - This is now stale. It should be replaced with a hard diagnostic that bare label blocks are invalid and that keyword blocks must be used.
- `src/compiler_frontend/hir/hir_statement.rs`
  - HIR statement lowering dispatches on `NodeKind`.
  - A new AST block node needs a lowering case.
- `src/compiler_frontend/hir/hir_nodes.rs`
  - HIR already has `RegionId` / `HirRegion`, so plain scoped blocks should eventually become real lexical lifetime boundaries rather than only AST-only grouping.
- `Cargo.toml`
  - Current features are debug/display-oriented.
  - A new feature gate can be added for the new syntax.

## Keyword decision

Reserve these keywords now:

```text
block
checked
async
```

Use `block` for ordinary lexical grouping.

Do **not** reserve `scope` unless it becomes the final chosen spelling. Reserving both `block` and `scope` weakens Beanstalk's “one keyword per concept” direction and creates avoidable name churn.

## Syntax contract

### Accepted when syntax gate is enabled

```beanstalk
block:
    local_value = 1
    io(local_value)
;
```

### Reserved but not semantically implemented yet

```beanstalk
checked:
    ...
;
```

```beanstalk
async:
    ...
;
```

These should produce clear deferred-feature diagnostics until their semantics are implemented.

### Rejected

```beanstalk
some_name:
    ...
;
```

Diagnostic should say this is not label syntax and that blocks require a reserved keyword:

```text
Bare labeled blocks are not valid Beanstalk syntax.
Use `block:` for an ordinary scoped block, or one of the reserved block forms when enabled.
```

### Also rejected

```beanstalk
value: Int = 1
```

Diagnostic should explain that Beanstalk does not use `name: Type` declarations:

```text
Beanstalk declarations do not use `name: Type`.
Write `value Int = 1` instead.
```

This is important because `symbol : type` is a common habit from TypeScript, Python, Kotlin, Swift, etc.

## Implementation plan

### 1. Add token variants

File:

```text
src/compiler_frontend/tokenizer/tokens.rs
```

Add variants near control-flow / block syntax:

```rust
// Statement blocks
Block,
Checked,
Async,
```

Suggested placement: after `Return` or before `Loop`, because these are statement-position keywords.

### 2. Tokenize the new keywords

File:

```text
src/compiler_frontend/tokenizer/lexer.rs
```

In `keyword_or_variable()`, add:

```rust
"block" => return_token!(TokenKind::Block, stream),
"checked" => return_token!(TokenKind::Checked, stream),
"async" => return_token!(TokenKind::Async, stream),
```

These should be tokenized unconditionally. Reservation should not depend on a feature flag.

Rationale: once reserved, these words should stop being valid identifiers everywhere. That prevents source compatibility churn later.

### 3. Update reserved keyword shadow policy

File:

```text
src/compiler_frontend/symbols/identifier_policy.rs
```

Add:

```rust
"block", "checked", "async"
```

to `RESERVED_KEYWORD_SHADOWS`.

Also update the fixed array length.

Add/extend tests in:

```text
src/compiler_frontend/symbols/identifier_policy_tests.rs
```

Expected rejected examples:

```text
block
Block
_BLOCK
checked
Checked
_async
```

The current policy ignores case and leading underscores, so all of those should shadow reserved keywords.

### 4. Add a syntax feature gate

File:

```text
Cargo.toml
```

Add a language-surface feature:

```toml
statement_blocks = []
```

This should gate parsing/lowering of `block:` only.

Do not gate keyword reservation.

Recommended behavior:

| Syntax | Without `statement_blocks` | With `statement_blocks` |
|---|---|---|
| `block:` | reserved feature diagnostic | parsed as plain scoped block |
| `checked:` | deferred feature diagnostic | still deferred |
| `async:` | deferred feature diagnostic | still deferred |

`checked:` and `async:` should stay reserved-but-disabled until their dedicated work starts. Do not silently parse them as plain blocks.

### 5. Add AST block kind

File:

```text
src/compiler_frontend/ast/ast_nodes.rs
```

Add a small enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementBlockKind {
    Block,
    Checked,
    Async,
}
```

Add a node:

```rust
ScopedBlock {
    kind: StatementBlockKind,
    body: Vec<AstNode>,
},
```

Place it near other control-flow-ish `NodeKind` variants, probably after `Match` or before loops.

Even though only `Block` lowers now, carrying the kind is useful because:
- parsing can share a block parser,
- future diagnostics can preserve source intent,
- future HIR lowering can specialize `checked` / `async` cleanly.

### 6. Add a block parser module

New file:

```text
src/compiler_frontend/ast/statements/scoped_blocks.rs
```

Responsibilities:

- Consume the block keyword.
- Require immediate `:`.
- Parse nested statements until `;`.
- Create a child scope.
- Return `NodeKind::ScopedBlock`.

Suggested API:

```rust
pub(crate) fn parse_scoped_block_statement(
    token_stream: &mut FileTokens,
    parent_context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
    kind: StatementBlockKind,
) -> Result<AstNode, CompilerError>
```

Expected structure:

```rust
let location = token_stream.current_location();

// consume `block` / `checked` / `async`
token_stream.advance();

if token_stream.current_token_kind() != &TokenKind::Colon {
    return_syntax_error!(
        format!("Expected ':' after '{}' block keyword.", kind.keyword()),
        token_stream.current_location(), {
            CompilationStage => "AST Construction",
            PrimarySuggestion => format!("Write '{}:' to start this block", kind.keyword()),
            SuggestedInsertion => ":",
        }
    );
}

token_stream.advance();

let child_context = parent_context.new_child_control_flow(ContextKind::Block, string_table);
let body = parse_function_body_statements(
    token_stream,
    child_context,
    warnings,
    string_table,
)?;

Ok(AstNode {
    kind: NodeKind::ScopedBlock { kind, body },
    location,
    scope: parent_context.scope.clone(),
})
```

A better follow-up is to rename `new_child_control_flow()` or add `new_child_scope()` because plain blocks are not control flow. For the first implementation, reusing it is acceptable if no control-flow-only assumptions are tied to the helper.

### 7. Add `ContextKind::Block`

File:

```text
src/compiler_frontend/ast/module_ast/scope_context.rs
```

Add:

```rust
Block,
```

to `ContextKind`.

A plain block:
- should not increment `loop_depth`,
- should create a fresh child scope path,
- should inherit visible declarations and top-level context,
- should not leak local declarations back to the parent parser context.

Current `new_child_control_flow()` already increments loop depth only for `ContextKind::Loop`, so adding `Block` should work as a child scope kind.

### 8. Wire statement dispatch

File:

```text
src/compiler_frontend/ast/statements/body_dispatch.rs
```

Add imports:

```rust
use crate::compiler_frontend::ast::ast_nodes::StatementBlockKind;
use crate::compiler_frontend::ast::statements::scoped_blocks::parse_scoped_block_statement;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
```

Add match arms:

```rust
TokenKind::Block => {
    if !cfg!(feature = "statement_blocks") {
        return Err(deferred_feature_rule_error(
            "`block:` scoped blocks are reserved but currently feature-gated.",
            token_stream.current_location(),
            "AST Construction",
            "Enable the `statement_blocks` feature or remove the block.",
        ));
    }

    ast.push(parse_scoped_block_statement(
        token_stream,
        &context,
        warnings,
        string_table,
        StatementBlockKind::Block,
    )?);
}
```

For `checked`:

```rust
TokenKind::Checked => {
    return Err(deferred_feature_rule_error(
        "`checked:` blocks are reserved for future advanced validation, but are not implemented yet.",
        token_stream.current_location(),
        "AST Construction",
        "Use `block:` for a normal scoped block, or remove the checked block until the feature is implemented.",
    ));
}
```

For `async`:

```rust
TokenKind::Async => {
    return Err(deferred_feature_rule_error(
        "`async:` blocks are reserved for future async lowering, but are not implemented yet.",
        token_stream.current_location(),
        "AST Construction",
        "Remove the async block until async lowering is implemented.",
    ));
}
```

These errors should fire even if the next token is not `:`. The keyword is reserved, so using it as a value/declaration name should not be allowed.

### 9. Replace stale labeled-scope diagnostic

File:

```text
src/compiler_frontend/declaration_syntax/type_syntax.rs
```

Current branch:

```rust
TokenKind::Colon if matches!(context, TypeAnnotationContext::DeclarationTarget) => {
    return Err(deferred_feature_rule_error(
        "Labeled scopes are deferred for Alpha.",
        ...
    ));
}
```

Replace it with a diagnostic that rejects bare labels and also catches `name: Type` mistakes.

Recommended wording:

```rust
TokenKind::Colon if matches!(context, TypeAnnotationContext::DeclarationTarget) => {
    return_syntax_error!(
        "Unexpected ':' after declaration name. Beanstalk does not support bare labeled blocks or `name: Type` declarations.",
        token_stream.current_location(), {
            CompilationStage => "Variable Declaration",
            PrimarySuggestion => "Use `block:` for a scoped block, or write declarations as `name Type = value`.",
        }
    );
}
```

This is the exact point where `symbol:` currently gets interpreted as “maybe labeled scope.” It should become the canonical diagnostic for old label-style block syntax and foreign `name: Type` declaration habits.

### 10. Add HIR lowering support for plain blocks

File:

```text
src/compiler_frontend/hir/hir_statement.rs
```

Add a match arm:

```rust
NodeKind::ScopedBlock {
    kind: StatementBlockKind::Block,
    body,
} => self.lower_scoped_block_statement(body, &node.location),
```

For now, `checked` and `async` should not reach HIR. If they do, return a compiler error.

Suggested helper:

```rust
fn lower_scoped_block_statement(
    &mut self,
    body: &[AstNode],
    location: &SourceLocation,
) -> Result<(), CompilerError> {
    self.lower_statement_sequence(body)
}
```

This gives correct basic execution order and keeps AST-level lexical visibility.

However, this is only a first pass. Because HIR already models `RegionId`, a stronger implementation should create a child lexical region for the block so borrow/drop analysis can treat the block as a real lifetime boundary. Add a TODO comment explaining this.

Recommended TODO:

```rust
// TODO: Lower plain scoped blocks through a child lexical RegionId so borrow/drop
// analysis can treat block locals as ending at the block boundary. The initial
// lowering preserves execution order and AST lexical visibility, but does not yet
// expose a separate HIR lifetime boundary.
```

If existing HIR builder utilities already support entering/leaving regions, use them instead of flattening.

### 11. Add syntax tests

Add integration tests under:

```text
tests/cases/
```

Suggested cases:

#### `block_scoped_local_success`

Input:

```beanstalk
block:
    value = "inside"
    io(value)
;
```

Expected:
- success when `statement_blocks` feature is enabled.
- If the integration runner does not support feature-specific tests yet, add parser/unit tests first and mark integration follow-up.

#### `block_local_does_not_escape`

Input:

```beanstalk
block:
    value = "inside"
;

io(value)
```

Expected:
- failure.
- Error should prove `value` is not visible outside the block.

#### `bare_label_block_rejected`

Input:

```beanstalk
setup:
    value = 1
;
```

Expected:
- failure.
- Message contains `Bare labeled blocks are not valid` or the new `Unexpected ':' after declaration name` wording.
- Suggests `block:`.

#### `typescript_style_declaration_rejected`

Input:

```beanstalk
value: Int = 1
```

Expected:
- failure.
- Message says Beanstalk does not use `name: Type`.
- Suggests `value Int = 1`.

#### `checked_block_reserved`

Input:

```beanstalk
checked:
    value = 1
;
```

Expected:
- failure.
- Message says `checked:` is reserved for future advanced validation and not implemented.

#### `async_block_reserved`

Input:

```beanstalk
async:
    value = 1
;
```

Expected:
- failure.
- Message says `async:` is reserved for future async lowering and not implemented.

#### `reserved_keywords_cannot_be_identifiers`

Inputs:

```beanstalk
block = 1
checked = 1
async = 1
```

Expected:
- failure from tokenizer/parser path because they are keywords, not symbols.
- Use clear diagnostics rather than falling through to generic unexpected-token errors where practical.

### 12. Add tokenizer/unit tests

File:

```text
src/compiler_frontend/tokenizer/tests/lexer_tests.rs
```

Add a test that confirms:

```text
block checked async
```

tokenizes as:

```rust
TokenKind::Block
TokenKind::Checked
TokenKind::Async
```

Also add tests that these no longer appear as `TokenKind::Symbol`.

### 13. Add docs cleanup task

Search/remove any documentation or planning text that says arbitrary labels are planned/supported:

```text
label:
name:
labeled scope
labeled block
unique name followed by a colon
```

Specific known cleanup from current repo state:

- Remove or replace the `Labeled scopes are deferred for Alpha.` wording in `type_syntax.rs`.
- The uploaded README only mentions `async` as unimplemented future syntax. That can stay, but should eventually point to the new block syntax docs once written.

The replacement docs should say:

```text
Statement blocks are keyword-gated. Bare labels are not valid Beanstalk syntax.
Use `block:` for an ordinary scoped block.
`checked:` and `async:` are reserved future block forms.
```

## Parser edge cases

### Missing colon after block keyword

Input:

```beanstalk
block
    value = 1
;
```

Expected diagnostic:

```text
Expected ':' after 'block' block keyword.
```

### Extra tokens before colon

Input:

```beanstalk
block name:
    value = 1
;
```

Expected diagnostic:

```text
Expected ':' after 'block'. Block keywords do not take names.
```

Implementation can initially report this as “Expected ':' after 'block' block keyword” at `name`, but a more specific message is better.

### `checked` or `async` used as names

Input:

```beanstalk
checked = true
async = false
```

Expected diagnostic:

```text
'checked' is a reserved keyword and cannot be used as a declaration name.
```

If this currently becomes “unexpected `Checked` in function body,” add targeted arms in `unexpected_function_body_token_error()` or in the main dispatch.

### Non-entry top-level block

Input in non-entry file:

```beanstalk
block:
    io("no")
;
```

Expected:
- same rule as other top-level executable statements.
- Non-entry files cannot contain top-level executable code.

Header parsing will currently collect unknown top-level tokens into the implicit start body and later reject non-entry executable start bodies, so this likely needs no special case.

## Recommended staged PR breakdown

### PR 1 — Reserve keywords and diagnostics

- Add token variants.
- Add lexer keyword mapping.
- Update `RESERVED_KEYWORD_SHADOWS`.
- Replace stale labeled-scope diagnostic.
- Add tokenizer and identifier-policy tests.
- Add reserved `checked` / `async` diagnostics in body dispatch.

No `block:` success path yet.

### PR 2 — Add feature-gated `block:` syntax

- Add `statement_blocks` Cargo feature.
- Add `StatementBlockKind`.
- Add `NodeKind::ScopedBlock`.
- Add `ContextKind::Block`.
- Add `scoped_blocks.rs`.
- Add `body_dispatch.rs` routing.
- Add HIR lowering for plain `Block`.
- Add parser/HIR tests.

### PR 3 — Strengthen HIR region semantics

- Audit HIR builder region utilities.
- Make `block:` create a child lexical region if the current builder supports it cleanly.
- Ensure borrow checker/drop analysis sees the block boundary.
- Add regression test where a block-local owned value reaches a block-exit drop point in ownership-enabled backends once that machinery exists.

## Acceptance criteria

- `block`, `checked`, and `async` are no longer valid identifiers.
- `block:` parses only when the feature gate is enabled.
- `checked:` and `async:` produce deferred-feature diagnostics.
- Bare label-style blocks are rejected.
- `name: Type` mistakes produce a helpful Beanstalk-specific diagnostic.
- Block-local variables do not leak into the parent AST parser context.
- Existing `if`, `loop`, `match`, template, declaration, and expression statement behavior is unchanged.
- `cargo clippy`, `cargo test`, and `cargo run tests` pass.