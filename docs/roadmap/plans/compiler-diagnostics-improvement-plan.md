# Beanstalk Compiler Diagnostics Improvement Plan

## Purpose

This plan collects concrete opportunities to improve user-facing compiler diagnostics across the Beanstalk frontend. Each entry is discovered by writing subtly wrong or dense Beanstalk code, running `bean check`, and evaluating whether the emitted diagnostic is specific, helpful, and actionable.

The plan is intentionally a research/audit deliverable, not an implementation task list. When this work is scheduled, each entry should become one or more integration test cases under `tests/cases/` plus the corresponding diagnostic refactor in `src/compiler_frontend/compiler_messages/` and the owning compiler stage.

## Diagnostic quality checklist

A good Beanstalk diagnostic should:

- Be simple to read but specific (name the exact token, type, name, or construct).
- Explain *why* the source is rejected in terms of the language rule.
- Provide a useful correction or next step when practical.
- Branch into more specific variants when one umbrella message covers too many distinct mistakes.
- Preserve source locations and use stable diagnostic codes.
- Stay on `CompilerDiagnostic` paths; never route user mistakes through `CompilerError`.

Tone may include concise, acerbic humour where it lightens the mood without obscuring the facts.

## How each entry is recorded

```markdown
### AREA-NNN: Short title

- **Stage:** tokenization / headers / dependency sort / AST / HIR / borrow / backend
- **Current code:** stable diagnostic code if known, or "unknown"
- **Snippet:** the Beanstalk source that triggers the weak diagnostic
- **Current diagnostic:** the rendered message or terse summary
- **Weakness:** why the current message is unhelpful or incomplete
- **Proposed diagnostic:** the improved message(s), possibly split into variants
- **Test coverage to add:** integration cases and any specific fixtures
```

---

## Found opportunities

### DIAG-001: Collection mutating methods do not suggest the `~` receiver syntax

- **Stage:** AST expression/type checking
- **Current code:** `BST-RULE-0047`
- **Snippet:**
  ```beanstalk
  values {Int} = {1, 2, 3}
  values.push(4)
  ```
- **Current diagnostic:** `Collection mutating method 'push' requires a mutable collection receiver.`
- **Weakness:** The message states the requirement but does not show the exact call-site fix. A user new to Beanstalk has to infer that `~values.push(4)` is the required spelling.
- **Proposed diagnostic:** `Collection mutating method 'push' requires a mutable receiver. Try '~values.push(4)'.` (Split into variants for missing `~` vs genuinely immutable binding if the distinction is not already handled.)
- **Test coverage to add:** `tests/cases/collection_mutator_missing_tilde_suggestion` (assert message contains the `~` prefix suggestion).

### DIAG-002: Generic constructor syntax rejected without guidance

- **Stage:** AST type/value checking
- **Current code:** `BST-RULE-0037`
- **Snippet:**
  ```beanstalk
  Box type A = | value A |
  x = Box of String { value = "hello" }
  ```
- **Current diagnostic:** `'Box' is a type and cannot be used as a value.`
- **Weakness:** The real mistake is using the generic type name as a constructor expression. The diagnostic does not explain that generic constructors need a receiving type annotation or use the concrete constructor syntax.
- **Proposed diagnostic:** Add a new rule variant for generic types used as constructors: `'Box of String' is a type, not a constructor expression. Declare the value with an explicit type annotation, e.g. 'x ~Box of String = ...', or use the inferred constructor form where the type is known from context.` (Keep the core message simple; this is a distinct case from ordinary value/type namespace misuse.)
- **Test coverage to add:** `tests/cases/generic_constructor_namespace_improvement` covering both the bare generic name and the `of` application cases.

### DIAG-003: Statement match arm with expression value reports parser error on the pattern

- **Stage:** Parser / AST match parsing
- **Current code:** `BST-SYNTAX-0002`
- **Snippet:**
  ```beanstalk
  Color ::
      Red,
      Green,
      Blue,
  ;
  
  color = Color::Red
  
  if color is:
      Red => 1
      Green => 2
  ;
  ```
- **Current diagnostic:** `Unexpected token name 'Green'.`
- **Weakness:** The error is reported on the second arm pattern, not on the value `1` or `2` where the actual mistake lives. The user wrote a statement match but supplied expression bodies. The message is cryptic because it looks like a parser bug.
- **Proposed diagnostic:** Detect statement-match arms whose body is a non-statement expression and report at the expression: `Statement match arms must contain statements, not bare expressions. Use 'then' for value-producing matches, e.g. 'Red => then 1'.` If the parser genuinely cannot recover, at least emit a clearer message like `Expected a statement body for this match arm; found an expression.`
- **Test coverage to add:** `tests/cases/statement_match_expression_body` and a positive case `tests/cases/value_match_then_required`.

### DIAG-004: Operator spacing diagnostic is one-size-fits-all

- **Stage:** Tokenization
- **Current code:** `BST-SYNTAX-0031`
- **Snippet:**
  ```beanstalk
  x = 1 +2
  ```
- **Current diagnostic:** `Symbolic binary operators require whitespace on both sides.`
- **Weakness:** The message is correct for `1+2` but misleading for `1 +2` where whitespace exists on one side. It does not name which side is missing or which operator is involved.
- **Proposed diagnostic:** Split into:
  - `Binary operator '+' needs whitespace after it as well.`
  - `Binary operator '+' needs whitespace before it as well.`
  - `Binary operator '+' needs whitespace on both sides.`
  Keep the umbrella code but add a reason/location detail that names the operator and the missing side.
- **Test coverage to add:** `tests/cases/binary_operator_one_sided_whitespace` covering left-only, right-only, and neither sides.

### DIAG-005: Compound assignment spacing misclassified as binary operator spacing

- **Stage:** Tokenization
- **Current code:** `BST-SYNTAX-0031`
- **Snippet:**
  ```beanstalk
  x ~= 1
  x +=2
  ```
- **Current diagnostic:** `Symbolic binary operators require whitespace on both sides.`
- **Weakness:** `+=` is not a binary operator; it is a compound assignment. The message suggests a fix for the wrong construct. The same applies to `-=`, `*=`, `/=`, `//=`, `%=`, and `^=`.
- **Proposed diagnostic:** New dedicated code or sub-variant: `Compound assignment '+= ' needs whitespace after the operator. Use 'x += 2'.` (Distinguish left-missing, right-missing, and both.)
- **Test coverage to add:** `tests/cases/compound_assignment_spacing` for each operator family and each missing side.

### DIAG-006: Constant fallible cast does not explain why the literal fails

- **Stage:** AST constant folding / cast resolution
- **Current code:** `BST-RULE-0083`
- **Snippet:**
  ```beanstalk
  value = "not a number"
  number Int = cast value
  ```
- **Current diagnostic:** `` `cast` selected fallible evidence. Use `cast!` or `cast ... catch:`. ``
- **Weakness:** When the source is a constant literal that can be evaluated at compile time, the generic fallible-cast message is unhelpful. The compiler already knows the literal is unparseable; it should say so.
- **Proposed diagnostic:** Add a constant-evaluation branch: `Cannot cast the string literal "not a number" to Int: it is not a valid Beanstalk integer literal.` Suggest `cast!` or `catch` only when the source is not statically known to fail.
- **Test coverage to add:** `tests/cases/cast_const_literal_unparseable` for string-to-int, string-to-float out of range, and float-to-int non-integer constants.

### DIAG-007: Immutable binding error shown for struct field mutation through immutable place

- **Stage:** AST l-value / assignment checking
- **Current code:** `BST-RULE-0044`
- **Snippet:**
  ```beanstalk
  Point = |
      x Int,
      y Int,
  |
  
  p = Point(x = 1, y = 2)
  p.x = 10
  ```
- **Current diagnostic:** `Cannot mutate immutable variable 'p'. Use '~' to declare a mutable variable.`
- **Weakness:** The user does not need to redeclare `p` as mutable; field mutation requires an explicit mutable/exclusive access marker on the field access itself: `~p.x = 10`. The current suggestion sends the user down the wrong path.
- **Proposed diagnostic:** `Cannot mutate field 'x' through immutable binding 'p'. Use '~p.x = 10' to request exclusive access for this write.` (Split from the variable-redeclaration case.)
- **Test coverage to add:** `tests/cases/struct_field_write_needs_tilde` and `tests/cases/struct_field_write_mutable_binding` to show the two distinct fixes.

### DIAG-008: Mutable receiver call suggestion uses internal parameter name

- **Stage:** AST receiver-call validation
- **Current code:** `BST-RULE-0047`
- **Snippet:**
  ```beanstalk
  Point = |
      x Int,
      y Int,
  |
  
  move |this ~Point, dx Int, dy Int|:
      this.x += dx
      this.y += dy
  ;
  
  p ~= Point(x = 1, y = 2)
  p.move(3, 4)
  ```
- **Current diagnostic:** `'move' expects mutable access at the receiver call site. Call this with '~this receiver'.`
- **Weakness:** `~this receiver` is not Beanstalk syntax; it is an internal description of the receiver parameter. The user needs to know to write `~p.move(3, 4)`.
- **Proposed diagnostic:** `Mutable receiver method 'move' requires '~' at the call site. Try '~p.move(3, 4)'.`
- **Test coverage to add:** `tests/cases/receiver_mutable_tilde_suggestion` with a message fragment assertion.

### DIAG-009: Unknown imported namespace member could suggest the namespace prefix

- **Stage:** AST name resolution
- **Current code:** `BST-RULE-0034`
- **Snippet:**
  ```beanstalk
  import @core/math
  
  value = pi
  ```
- **Current diagnostic:** `Unknown value name 'pi'.`
- **Weakness:** `pi` is available as `math.pi` because of the implicit namespace alias. The diagnostic should hint at the grouped import or the namespace-qualified access, especially when the name exists in an imported namespace.
- **Proposed diagnostic:** `Unknown value name 'pi'. Did you mean 'math.pi' from the imported '@core/math' namespace? Use a grouped import if you want to refer to it as 'pi'.` (Add a reason enum for `NameExistsInImportedNamespace`.)
- **Test coverage to add:** `tests/cases/import_namespace_member_did_you_mean` covering core and external package namespaces.

### DIAG-011: Malformed `$children(...)` argument crashes with an infrastructure error

- **Stage:** Template parser / AST template construction
- **Current code:** `BST-INFRA-0001`
- **Snippet:**
  ```beanstalk
  [: [$children(] ]
  ```
- **Current diagnostic:** `Infrastructure failure [BST-INFRA-0001] No nodes found in expression. This should never happen.`
- **Weakness:** A malformed user template should never surface as a compiler invariant failure. The diagnostic is routed through `CompilerError` and carries no useful source location or correction.
- **Proposed diagnostic:** Convert to a syntax/rule error: `Expected a template or string argument for $children(...).` with a secondary label pointing at the unclosed `(`.
- **Test coverage to add:** `tests/cases/template_children_malformed_argument` asserting a `CompilerDiagnostic` code (not infrastructure) and a stable syntax code.

### DIAG-012: Value-producing `if` missing `else` value crashes with an infrastructure error

- **Stage:** Parser / AST expression parsing
- **Current code:** `BST-INFRA-0001`
- **Snippet:**
  ```beanstalk
  x = if true then 1 else
  ```
- **Current diagnostic:** `Infrastructure failure [BST-INFRA-0001] No nodes found in expression. This should never happen.`
- **Weakness:** A trailing `else` with no value is a common user mistake, not a compiler bug. It should be reported as a syntax error with a clear fix.
- **Proposed diagnostic:** `Expected a value after 'else'. A value-producing 'if' needs both 'then' and 'else' branches, e.g. 'if true then 1 else 0'.`
- **Test coverage to add:** `tests/cases/value_if_missing_else_value`.

### DIAG-013: Terse renderer drops the message for `export`-outside-facade errors

- **Stage:** Diagnostic rendering (terse)
- **Current code:** `BST-RULE-0077`
- **Snippet:**
  ```beanstalk
  export value #= 1
  ```
- **Current diagnostic (terse):** `E|BST-RULE-0077|export_outside_facade.bst|1:2|` (empty message field)
- **Current diagnostic (terminal):** `` `export` is only valid in `#mod.bst`; expose declarations through the nearest module facade ``
- **Weakness:** The descriptor has a title, but the terse renderer emits an empty message. This breaks tooling and CI that parse terse output.
- **Proposed diagnostic:** Ensure the terse renderer falls back to the descriptor title when the diagnostic payload does not carry a custom rendered message. Add a renderer regression test.
- **Test coverage to add:** A unit test in `src/compiler_frontend/compiler_messages/tests/` that renders every `RuleDiagnosticKind` with the terse renderer and asserts no empty message strings.

### DIAG-014: Template head "incompatible item" error does not name the conflict

- **Stage:** Template parser / AST template construction
- **Current code:** `BST-SYNTAX-0022`
- **Snippet:**
  ```beanstalk
  [: [$md, $raw: hello] ]
  ```
- **Current diagnostic:** `This template head item is incompatible with other meaningful items in this template head.`
- **Weakness:** The message does not say which directives conflict or why `$md` and `$raw` cannot coexist. The user has to guess.
- **Proposed diagnostic:** Name the incompatible pair: `Cannot combine '$md' and '$raw' in the same template head; they are mutually exclusive formatting directives. Choose one.`
- **Test coverage to add:** `tests/cases/template_head_conflicting_directives`.

### DIAG-015: Unary `not` on non-Bool does not name the operator

- **Stage:** AST type checking
- **Current code:** `BST-TYPE-0003`
- **Snippet:**
  ```beanstalk
  x = 1
  if not x:
      io.line("not one")
  ;
  ```
- **Current diagnostic:** `Unsupported operand type for unary operator. Operand: Int.`
- **Weakness:** The operator is `not`, which is a word, not a symbol. The message should name it explicitly so the user does not think the issue is with arithmetic negation.
- **Proposed diagnostic:** `Operator 'not' requires a Bool operand, found Int. Use a comparison if you meant to test equality, e.g. 'x is 1'.`
- **Test coverage to add:** `tests/cases/not_operator_non_bool`.

### DIAG-016: Truncated `if` condition reports generic EOF

- **Stage:** Parser
- **Current code:** `BST-SYNTAX-0002`
- **Snippet:**
  ```beanstalk
  x = if
  ```
- **Current diagnostic:** `Unexpected token end of file.`
- **Weakness:** The `if` keyword is present but no condition follows. The error is generic and does not guide the user.
- **Proposed diagnostic:** `Expected a condition after 'if'.` with a secondary label on the `if` token.
- **Test coverage to add:** `tests/cases/if_missing_condition`.

### DIAG-017: Struct extra delimiter error misidentifies the token

- **Stage:** Parser
- **Current code:** `BST-SYNTAX-0030`
- **Snippet:**
  ```beanstalk
  Person = |
      name String,
  | |
  ```
- **Current diagnostic:** `Unexpected '/' in function body. '/' is valid in function signatures, struct field/type declarations, and loop binding headers.`
- **Weakness:** The token is `|`, not `/`. The message is confusing and suggests the parser is in a function body state when it should be in a struct-trailer state. The user gets no guidance on how to close the struct correctly.
- **Proposed diagnostic:** `Unexpected '|' after the closing '|' of struct 'Person'. Structs are closed with a single '|' and do not use ';'.` (or whatever the actual terminator is; verify against the grammar.)
- **Test coverage to add:** `tests/cases/struct_extra_delimiter`.

### DIAG-018: Truncated field access says "found this field"

- **Stage:** Parser / AST field access
- **Current code:** `BST-RULE-0048`
- **Snippet:**
  ```beanstalk
  x = 1
  y = x.
  ```
- **Current diagnostic:** `Expected property or method name after '.', found this field.`
- **Weakness:** "found this field" is not a meaningful token description; it reads like a placeholder. The error should say "found end of file" or "found newline".
- **Proposed diagnostic:** `Expected a property or method name after '.', but the expression ends here.`
- **Test coverage to add:** `tests/cases/field_access_truncated`.

### DIAG-019: Type alias collection shorthand missing `as` is misreported as uninitialized variable

- **Stage:** Parser / AST declaration parsing
- **Current code:** `BST-RULE-0031`
- **Snippet:**
  ```beanstalk
  MyList {Int}
  
  x MyList = {1, 2, 3}
  ```
- **Current diagnostic:** `Uninitialized variable 'MyList'`
- **Weakness:** `MyList {Int}` is not a variable declaration; it is a malformed type alias. The parser treats `MyList` as a variable and `{Int}` as a collection literal, then complains it has no initializer. The user is sent to fix the wrong construct.
- **Proposed diagnostic:** `Expected 'as' in type alias declaration. Use 'MyList as {Int}'.` Detect the pattern `Name {Type}` at top level where a declaration is expected.
- **Test coverage to add:** `tests/cases/type_alias_collection_shorthand_missing_as`.

### DIAG-020: `copy` on a mutable place is rejected as a literal/computed expression

- **Stage:** AST copy validation
- **Current code:** `BST-RULE-0056`
- **Snippet:**
  ```beanstalk
  x ~= 1
  y = copy ~x
  ```
- **Current diagnostic:** `The 'copy' keyword requires a variable or field, not a literal or computed expression. Assign the value to a variable first, then copy it, for example 'tmp = value' followed by 'copy tmp'.`
- **Weakness:** `~x` is a variable access with a mutable marker, not a literal or computed expression. The diagnostic is factually wrong. `copy` should either accept mutable places or report that the marker is not allowed.
- **Proposed diagnostic:** If `copy` is meant to be marker-free, report `copy does not take '~'. Use 'copy x' to copy the value of a mutable binding.` If mutable places are intentionally rejected, report that specifically instead of calling them literals.
- **Test coverage to add:** `tests/cases/copy_mutable_place`.

### DIAG-021: `~` on assignment target is misreported as receiver-only syntax

- **Stage:** Parser / AST assignment parsing
- **Current code:** `BST-RULE-0047`
- **Snippet:**
  ```beanstalk
  x = 1
  ~x = 2
  ```
- **Current diagnostic:** `Mutable receiver marker '~' is only valid for receiver calls like '~value.method(...)' or '~values.push(...)'.`
- **Weakness:** The user is trying to mutate an existing immutable binding with `~x = 2`, which is not valid. The correct fix is `x ~= 2` (reassign a mutable binding) or recognizing that `x` is immutable. The current message sends them to receiver-call syntax.
- **Proposed diagnostic:** `The '~' marker is not valid on an assignment target. To declare or reassign a mutable binding, use 'x ~= 2'.` (Keep the receiver-only error as a separate variant for actual call sites.)
- **Test coverage to add:** `tests/cases/tilde_on_assignment_target`.

### DIAG-022: Grouped import of external namespace member says "symbol not found"

- **Stage:** Header import resolution
- **Current code:** `BST-IMPORT-0013`
- **Snippet:**
  ```beanstalk
  import @core/math
  
  import @core/math { pi }
  ```
- **Current diagnostic:** `Cannot import 'pi' from package '@core/math': symbol not found.`
- **Weakness:** `pi` exists as a member of the `@core/math` namespace, but grouped imports from external packages only work for direct exports, not for namespace members. The error should explain the distinction and suggest `import @core/math as calculus` then `calculus.pi`.
- **Proposed diagnostic:** `Cannot import 'pi' via grouped import: it is a namespace member, not a direct export of '@core/math'. Use 'import @core/math as calculus' and refer to 'calculus.pi'.`
- **Test coverage to add:** `tests/cases/external_package_grouped_namespace_member`.

### DIAG-023: Reactive declaration re-use reports "Unexpected token `$`"

- **Stage:** Parser / AST declaration parsing
- **Current code:** `BST-SYNTAX-0002`
- **Snippet:**
  ```beanstalk
  name $= "hello"
  
  name $= "world"
  ```
- **Current diagnostic:** `Unexpected token '$'.`
- **Weakness:** The second `$` is perfectly valid syntax; the real problem is that `name` is already declared. The error message is misleading and points at the wrong token.
- **Proposed diagnostic:** `Cannot redeclare reactive binding 'name'. Reactive sources must be uniquely named in a scope; rename the second declaration.`
- **Test coverage to add:** `tests/cases/reactive_redeclaration`.

### DIAG-024: Value-producing `if` missing `else` branch reports generic EOF

- **Stage:** Parser
- **Current code:** `BST-SYNTAX-0002`
- **Snippet:**
  ```beanstalk
  x = if true then 1
  ```
- **Current diagnostic:** `Unexpected token end of file.`
- **Weakness:** The parser knows an `if` is value-producing and needs an `else`. The generic EOF message hides this.
- **Proposed diagnostic:** `Value-producing 'if' requires an 'else' branch. Use 'if true then 1 else 0'.`
- **Test coverage to add:** `tests/cases/value_if_missing_else`.

### DIAG-025: Postfix `?` on error-returning function is misreported as unhandled error

- **Stage:** AST result handling
- **Current code:** `BST-RULE-0051`
- **Snippet:**
  ```beanstalk
  parse || -> Int, Error!:
      return! Error("bad")
  ;
  
  x = parse()?
  ```
- **Current diagnostic:** `Calls to error-returning functions must be explicitly handled with postfix `!` or `catch`.`
- **Weakness:** The call *is* explicitly handled with `?`, but `?` is the option-unwrap operator, not the error-propagation operator. The message conflates unhandled error propagation with wrong operator choice.
- **Proposed diagnostic:** `Postfix '?' unwraps optional values, but 'parse()' returns 'Int, Error!'. Use '!' to propagate the error, or 'catch' to recover.`
- **Test coverage to add:** `tests/cases/postfix_question_on_error_return`.

### DIAG-026: Catch recovery type mismatch is reported in a generic context

- **Stage:** AST result handling
- **Current code:** `BST-TYPE-0001`
- **Snippet:**
  ```beanstalk
  parse || -> Int, Error!:
      return! Error("bad")
  ;
  
  x = parse() catch:
      then "fallback"
  ;
  ```
- **Current diagnostic:** `Type mismatch in general: expected Int, found String`
- **Weakness:** The context is a `catch` recovery block, but the message says "in general". This makes it harder to see that the `then` value is the culprit.
- **Proposed diagnostic:** `Type mismatch in catch recovery: expected Int, found String. The 'then' value must match the success return type of the fallible call.`
- **Test coverage to add:** `tests/cases/catch_recovery_type_mismatch`.

### DIAG-010: Unclosed template EOF message does not name the construct

- **Stage:** Tokenization / parser
- **Current code:** `BST-SYNTAX-0017`
- **Snippet:**
  ```beanstalk
  [if true:
      hello
  ```
- **Current diagnostic:** `Unexpected end of file, expected ']'`
- **Weakness:** Correct but bare. It could mention that a template is unclosed and show the opening delimiter location.
- **Proposed diagnostic:** `This template started with '[' on line 1 is not closed. Add ']' before the end of the file.` (Secondary label on the opening bracket.)
- **Test coverage to add:** `tests/cases/unclosed_template_eof_location`.

### DIAG-027: Terse renderer drops messages for several syntax diagnostics

- **Stage:** Diagnostic rendering (terse)
- **Current codes:** `BST-SYNTAX-0006`, `BST-SYNTAX-0009`, and likely others
- **Snippet:**
  ```beanstalk
  message = "hello
  ```
- **Current diagnostic (terse):** `E|BST-SYNTAX-0006|...|1:12|` (empty message field)
- **Current diagnostic (terminal):** `Unterminated string literal`
- **Weakness:** The terse renderer is not falling back to the descriptor title for these diagnostics. The same issue likely affects `BST-SYNTAX-0009` (invalid char literal) and possibly other syntax kinds where the payload omits a custom message.
- **Proposed diagnostic:** Fix the terse renderer to always emit the descriptor title when no payload message is present. Add a unit test that iterates every `SyntaxDiagnosticKind` and asserts non-empty terse output.
- **Test coverage to add:** Unit test in `src/compiler_frontend/compiler_messages/tests/` plus `tests/cases/unclosed_string_terse` and `tests/cases/invalid_char_literal_terse`.

### DIAG-028: Invalid string escape sequences are silently accepted

- **Stage:** Tokenization
- **Current code:** none (no diagnostic emitted)
- **Snippet:**
  ```beanstalk
  message = "hello \q world"
  ```
- **Current diagnostic:** No error or warning.
- **Weakness:** Beanstalk string literals support escapes, but `\q` is not a valid escape. The tokenizer silently accepts it, which can lead to surprising runtime strings.
- **Proposed diagnostic:** `Invalid escape sequence '\q' in string literal. Supported escapes are \n, \t, \r, \\, \", etc.` (List the supported set.)
- **Test coverage to add:** `tests/cases/string_invalid_escape`.

### DIAG-029: Raw string with backslash-newline reports invalid character on the backtick

- **Stage:** Tokenization
- **Current code:** `BST-SYNTAX-0007`
- **Snippet:**
  ```beanstalk
  text = `hello \n world`
  ```
- **Current diagnostic:** `Invalid character: '`'
- **Weakness:** The error points at the backtick delimiter, not at the `\n` escape, and the message is confusing because raw strings do use backticks. Raw strings should reject escapes or treat them literally; the current message does not explain either.
- **Proposed diagnostic:** Either allow `\n` literally in raw strings with no diagnostic, or emit `Raw string literals do not process escape sequences; '\n' is preserved literally.`
- **Test coverage to add:** `tests/cases/raw_string_escape_behavior`.

### DIAG-030: String concatenation with non-String operand is called "arithmetic"

- **Stage:** AST type checking
- **Current code:** `BST-TYPE-0003`
- **Snippet:**
  ```beanstalk
  x = 1
  message = "value: " + x
  ```
- **Current diagnostic:** `Unsupported operand types for arithmetic operator. Left: String, Right: Int.`
- **Weakness:** `+` between strings is concatenation, not arithmetic. The message suggests a numeric operation when the issue is mixing a non-String into a concatenation.
- **Proposed diagnostic:** `Cannot concatenate String and Int. Use a template '[: value: [x]]' or 'cast x' to String first.`
- **Test coverage to add:** `tests/cases/string_concat_non_string`.

### DIAG-031: Multi-return assigned to a single target is silently accepted

- **Stage:** AST assignment / multi-bind validation
- **Current code:** none
- **Snippet:**
  ```beanstalk
  pair || -> String, Int:
      return "a", 1
  ;
  
  x = pair()
  ```
- **Current diagnostic:** No error or warning.
- **Weakness:** The docs state that regular declarations are single-target. Silently dropping the second return value is a bug and likely masks mistakes.
- **Proposed diagnostic:** `Cannot assign a multi-return function to a single target. Use 'x, y = pair()' or ignore the extra value explicitly.`
- **Test coverage to add:** `tests/cases/multi_return_single_target_rejected`.

### DIAG-032: `then` outside value block at top level gives generic unexpected token

- **Stage:** Parser
- **Current code:** `BST-SYNTAX-0002`
- **Snippet:**
  ```beanstalk
  x = if true then 1 else 2
  y = then 3
  ```
- **Current diagnostic:** `Unexpected token `then`.`
- **Weakness:** The function-body equivalent already has a good message: `` `then` is only valid inside a value-producing block. `` The top-level parser should use the same dedicated diagnostic.
- **Proposed diagnostic:** `` `then` is only valid inside a value-producing block (value-producing `if`, full match, or `catch` recovery). ``
- **Test coverage to add:** `tests/cases/then_outside_value_block_top_level`.

### DIAG-033: Generic function missing `type` keyword is reported as unknown type

- **Stage:** Parser / AST generic parameter resolution
- **Current code:** `BST-RULE-0035`
- **Snippet:**
  ```beanstalk
  make_box |value A| -> Box of A:
      return Box of A { value = value }
  ;
  ```
- **Current diagnostic:** `Unknown type name 'A'.`
- **Weakness:** The real issue is a missing `type` parameter declaration, not an unknown type. The user may not realize they wrote `make_box` instead of `make_box type A`.
- **Proposed diagnostic:** `Type name 'A' is not in scope. If this is a generic parameter, declare it with 'type': 'make_box type A |value A| -> Box of A'.`
- **Test coverage to add:** `tests/cases/generic_function_missing_type_keyword`.

### DIAG-034: Binary operator missing right-hand operand reports "missing left-hand operand"

- **Stage:** Parser / AST expression parsing
- **Current code:** `BST-SYNTAX-0024`
- **Snippet:**
  ```beanstalk
  x = 1
  y = x +
  ```
- **Current diagnostic:** `Missing left-hand operand for operator '+'.`
- **Weakness:** The left-hand operand `x` is present; the right-hand operand is missing. The error message is factually backwards.
- **Proposed diagnostic:** `Missing right-hand operand for operator '+'.` (or `Expected an expression after '+'.`)
- **Test coverage to add:** `tests/cases/binary_operator_missing_rhs`.

### DIAG-035: Duplicate function parameter names crash with infrastructure error

- **Stage:** Parser / AST function signature validation
- **Current code:** `BST-INFRA-0001`
- **Snippet:**
  ```beanstalk
  to_string |x Int, x String|:
      io.line(x)
  ;
  ```
- **Current diagnostic:** `Infrastructure failure [BST-INFRA-0001] Local 'x' is already declared in this function scope`
- **Weakness:** A duplicate parameter is a user-facing syntax/rule error, not a compiler invariant failure. It should be reported as a `CompilerDiagnostic` with a stable rule code.
- **Proposed diagnostic:** New rule code: `Duplicate parameter name 'x' in function 'to_string'. Parameter names must be unique within a signature.`
- **Test coverage to add:** `tests/cases/function_duplicate_parameter`.

### DIAG-036: Map literal missing value reports the same message as mixed entries

- **Stage:** Parser / AST map literal parsing
- **Current code:** `BST-SYNTAX-0033`
- **Snippet:**
  ```beanstalk
  scores {String = Int} = {"ada" = 10, "bob"}
  ```
- **Current diagnostic:** The mixed-entry form reports "Map literal entries must all use `key = value` syntax. Mixed collection and map entries are not allowed." A map entry that ends after `=` reports "Map literal entry is missing a value expression after '='."
- **Weakness:** The implementation now distinguishes the two parser branches, so the original entry must not regress to one umbrella message. The missing-value branch still does not name the key or show the exact repair.
- **Proposed diagnostic:** Keep the branches separate. Retain the mixed-entry message, and refine the missing-value branch to `Map entry for key "ada" is missing a value. Write '"ada" = 20'.`
- **Test coverage to add:** `tests/cases/map_literal_mixed_entries` and `tests/cases/map_literal_missing_value`, asserting distinct stable reason variants and message fragments.

### DIAG-038: Import path missing `@` is misreported as binary operator spacing

- **Stage:** Header import parsing
- **Current code:** `BST-SYNTAX-0031`
- **Snippet:**
  ```beanstalk
  import core/math
  ```
- **Current diagnostic:** `Symbolic binary operators require whitespace on both sides.`
- **Weakness:** The `/` in an import path is not a binary operator. The user simply forgot the `@` prefix. The error message is irrelevant.
- **Proposed diagnostic:** `Import paths must start with '@'. Did you mean 'import @core/math'?`
- **Test coverage to add:** `tests/cases/import_missing_at_prefix`.

### DIAG-039: Remove legacy `#import` compatibility instead of maintaining a diagnostic

- **Stage:** Tokenization / headers
- **Current code:** `BST-RULE-0025`
- **Snippet:**
  ```beanstalk
  #import @core/math
  ```
- **Current diagnostic:** "Legacy `#import` syntax is no longer supported" with no useful message body.
- **Weakness:** This is obsolete compatibility surface, not a diagnostic that should be expanded. The old syntax, diagnostic kind, renderer text, tests, and references should disappear together.
- **Proposed correction:** Remove the legacy `#import` parser path and all references to its diagnostic. Keep only current `import` syntax and its focused import diagnostics.
- **Test coverage to add:** Remove legacy `#import` fixtures and add a source-tree test or repository search gate proving that the old token, stable code, and enum variant are absent. Keep modern import coverage under `tests/cases/`.

### DIAG-040: Import of external namespace member says "Unknown value name"

- **Stage:** AST name resolution / external packages
- **Current code:** `BST-RULE-0034`
- **Snippet:**
  ```beanstalk
  import @core/math
  
  x = math.pi
  ```
- **Current diagnostic:** `Unknown value name 'pi'.`
- **Weakness:** `pi` is a member of the `@core/math` namespace, but the current access pattern is not supported. The error does not explain *why* it is unknown or how to access it correctly.
- **Proposed diagnostic:** Once the supported access pattern is known, tailor the message: e.g. `External package constants are not accessed through namespace members. Use a grouped import: 'import @core/math { pi }'.` (Verify the exact supported syntax before implementing.)
- **Test coverage to add:** `tests/cases/external_namespace_member_access`.

DIAG-042: @bst.sig before an unexported JavaScript function hides the missing export

* Stage: HTML external-JS annotation scanning and binding
* Current code: BST-IMPORT-0022
* Snippet:

/**
 * @bst.sig add |left Int, right Int| -> Int
 */
function add(left, right) {
    return left + right
}

* Current diagnostic: `@bst.sig` for `add` is not followed by a supported JS export declaration.
* Confirmed cause: The export scanner records only export function and export const ... => {} declarations. Plain functions and plain arrow-function constants are invisible to annotation binding. The binder then searches for any later supported export rather than the nearest top-level declaration.
* Weakness: The diagnostic does not identify the obvious missing keyword. Worse, when another export appears later in the file, the annotation can bind to that later export, causing misleading arity, duplicate-name or orphaned-annotation errors.
* Proposed correction:
    * Extend scanning to produce an ordered top-level declaration stream containing supported exported declarations and supported-looking unexported declarations.
    * Bind each @bst.sig to the nearest following top-level declaration, allowing only whitespace and comments between them.
    * Add a distinct parser reason such as MissingExportKeyword.
    * When the matched declaration is an unexported function, report: `@bst.sig` for `add` applies to JavaScript function `add`, but the function is not exported. Add `export` before `function`.
    * Provide the equivalent Add 'export' before 'const' correction for block-bodied arrow functions.
    * Place the primary label at the declaration’s insertion point and a secondary label on the @bst.sig.
    * Consume the unexported declaration after reporting it so the annotation cannot drift onto a later export.
    * Retain MissingExportAfterSig for genuinely orphaned annotations where no supported declaration follows. Improve that message to state the two accepted forms explicitly.
    * Preserve the JS parser reason through provider conversion instead of reducing every external-library failure to an opaque message string. A typed InvalidExternalLibraryReason under BST-IMPORT-0022 is sufficient unless separate stable codes are desired.
* Test coverage to add:
    * @bst.sig followed by a plain function reports only MissingExportKeyword.
    * @bst.sig followed by a plain block-bodied arrow constant reports the export const correction.
    * A missing-export function followed by a correctly annotated export does not shift either annotation.
    * A private helper declaration between an annotation and an export prevents distant binding.
    * A genuinely orphaned annotation continues to report MissingExportAfterSig.
    * Provider-level rendering preserves BST-IMPORT-0022, the targeted message and the JavaScript source span.

### DIAG-043: `NotResultExpression` hardcodes postfix `!` for every invalid handler

- **Stage:** AST result and option handling
- **Current code:** `BST-RULE-0051`
- **Snippets:**
  ```beanstalk
  value = 1 catch then 2
  ```
  ```beanstalk
  value String? = "text"
  unwrapped = value!
  ```
  ```beanstalk
  value String? = "text"
  fallback = value catch then "fallback"
  ```
- **Current diagnostic:** All three forms report `The '!' result-handling suffix is only valid for Result-valued expressions.`
- **Weakness:** `catch` on an ordinary value and `!` on an optional value are different mistakes with different repairs. The message names the wrong suffix for two of the three cases and calls the language's `Error!` channel a Result.
- **Proposed diagnostic:** Branch on the handler suffix and operand type with structured payload facts:
  - `Cannot use 'catch' on a non-fallible expression. 'catch' handles expressions that can return Error!.`
  - `Postfix '!' propagates Error! values, but this expression has type String?. Inspect the option with 'if value is |present| ...' instead.`
  - `Cannot use 'catch' to recover an optional value. Use 'if value is |present| then present else fallback'.`
- **Test coverage to add:** `tests/cases/catch_non_fallible_expression`, `tests/cases/postfix_bang_optional_expression`, and `tests/cases/catch_optional_expression`. Assert separate structured reasons and message fragments for `catch`, `!`, and optional operands.

### DIAG-044: Top-level `!` propagation says "surrounding function" and gives no local fix

- **Stage:** AST result handling
- **Current code:** `BST-RULE-0051`
- **Snippet:**
  ```beanstalk
  parse || -> Int, Error!:
      return 1
  ;

  value = parse()!
  ```
- **Current diagnostic:** `This expression uses '!' propagation, but the surrounding function does not declare an error return slot.`
- **Weakness:** The failing expression is in top-level code, so there is no surrounding function. The user needs the immediate alternative, not an abstract statement about a missing slot.
- **Proposed diagnostic:** `Cannot propagate '!' from top-level code because top-level code has no Error! return slot. Use 'parse() catch then fallback', or call it inside a function that returns an Error! slot.` Keep the non-error-function variant separate when a real enclosing function exists.
- **Test coverage to add:** `tests/cases/top_level_error_propagation` and `tests/cases/error_propagation_in_non_fallible_function`, asserting the top-level and nested-function variants point to the correct boundary and suggest `catch` or an `Error!` return slot.

### DIAG-045: Extra positional argument reports only the expected count

- **Stage:** AST call-shape validation
- **Current code:** `BST-RULE-0054`
- **Snippet:**
  ```beanstalk
  add |left Int, right Int| -> Int:
      return left + right
  ;

  value = add(1, 2, 3)
  ```
- **Current diagnostic:** `Call to 'add' provides more positional arguments than expected (expected 2).`
- **Weakness:** It omits the actual count and the correction. The user must count the call manually and infer which argument to remove.
- **Proposed diagnostic:** `Call to 'add' has 3 positional arguments, but the function accepts 2. Remove the extra argument, or use named arguments for the parameters you intend to pass.` Carry the found count and the extra argument location in the structured reason.
- **Test coverage to add:** `tests/cases/call_extra_positional_count` covering a user function, struct constructor, and choice constructor. Assert the actual and expected counts and the label on the extra argument.

### DIAG-046: Mutable call diagnostics do not distinguish an immutable binding from a missing marker

- **Stage:** AST call-shape and mutable-access validation
- **Current code:** `BST-RULE-0054`
- **Snippets:**
  ```beanstalk
  consume |values ~{Int}|:
      io.line("consumed")
  ;

  values {Int} = {}
  consume(values)
  ```
  ```beanstalk
  mutate |value ~Int|:
      value = 5
  ;

  value = 1
  mutate(~value)
  ```
- **Current diagnostic:** The first form reports "Call to 'consume' requires mutable access (~) for parameter 'values', but it was not provided. Prefix an existing mutable place with ~, for example `~value`." The second reports "...an immutable place was provided. Declare the binding with ~ to allow mutation."
- **Weakness:** In the first form, `values` is known to be immutable, so suggesting only `consume(~values)` leads directly to another error. The second form states the rule but does not show the declaration syntax or the corrected call together.
- **Proposed diagnostic:** Branch on place mutability:
  - `Call to 'consume' needs mutable access for 'values', but 'values' is immutable. Declare it as 'values ~{Int} = ...' and call 'consume(~values)'.`
  - `Cannot pass immutable 'value' as mutable access. Declare 'value ~Int = 1' before calling 'mutate(~value)'.`
- **Test coverage to add:** `tests/cases/mutable_call_missing_marker_on_immutable_binding` and `tests/cases/mutable_call_marker_on_immutable_binding`, plus a positive mutable-binding call to prevent the new branch from masking the normal missing-marker diagnostic.

### DIAG-047: Unknown type names do not suggest close builtin or visible type names

- **Stage:** AST type resolution
- **Current code:** `BST-RULE-0035`
- **Snippet:**
  ```beanstalk
  value Strng = "hello"
  ```
- **Current diagnostic:** `Unknown type name 'Strng'.`
- **Weakness:** `Strng` is a one-character typo for the builtin `String`, but the message offers no correction. Keep the existing DIAG-033 branch for unresolved generic parameters such as `A`.
- **Proposed diagnostic:** When a close visible type exists, report `Unknown type name 'Strng'. Did you mean 'String'?` Preserve the short generic message when no candidate is sufficiently close.
- **Test coverage to add:** `tests/cases/unknown_type_name_suggestion` for a close builtin typo, a close user-defined type typo, and an unrelated unknown name that must not receive a bogus suggestion.

### DIAG-048: Qualified namespace-member typos lose the namespace and available-member hint

- **Stage:** AST namespace/member resolution
- **Current code:** `BST-RULE-0034`
- **Snippet:**
  ```beanstalk
  value = io.lnie("hello")
  ```
- **Current diagnostic:** `Unknown value name 'lnie'.`
- **Weakness:** The diagnostic drops the receiver namespace and does not suggest the nearby builtin member `line`. This is different from DIAG-009, where an unqualified name should be qualified with an imported namespace.
- **Proposed diagnostic:** `Unknown member 'lnie' on namespace 'io'. Did you mean 'line'?` Keep a receiver-aware generic form for unrelated names, such as `Unknown member 'nope' on namespace 'io'.`
- **Test coverage to add:** `tests/cases/namespace_member_typo_suggestion` and `tests/cases/namespace_member_unknown_no_suggestion`, asserting the namespace is retained and suggestions are offered only for close candidates.

### DIAG-049: Unknown choice variants in match patterns omit the choice and available variants

- **Stage:** AST match-pattern resolution
- **Current code:** `BST-RULE-0049`
- **Snippet:**
  ```beanstalk
  Color ::
      Red,
      Blue,
  ;

  color = Color::Red
  if color is:
      Color::Green => io.line("green")
      else => io.line("other")
  ;
  ```
- **Current diagnostic:** `Unknown variant 'Green'.`
- **Weakness:** The message omits the scrutinee choice and gives no `Red`/`Blue` context. The separate choice-constructor renderer already has a useful suggestion policy, but match patterns do not use it.
- **Proposed diagnostic:** `Unknown variant 'Color::Green'. Did you mean 'Color::Red'? Available variants: [Red, Blue].` Carry the choice identity and visible variant list in the match-pattern payload. Do not suggest a variant for an unrelated name.
- **Test coverage to add:** `tests/cases/match_unknown_choice_variant_suggestion` and `tests/cases/match_unknown_choice_variant_without_suggestion`, with a unit renderer test for the close and unrelated candidate branches.

### DIAG-050: A mismatched match qualifier does not name either choice type

- **Stage:** AST match-pattern resolution
- **Current code:** `BST-RULE-0049`
- **Snippet:**
  ```beanstalk
  Color ::
      Red,
      Blue,
  ;
  Size ::
      Small,
      Large,
  ;

  color = Color::Red
  if color is:
      Size::Small => io.line("small")
      else => io.line("other")
  ;
  ```
- **Current diagnostic:** `Match arm qualifier does not match the scrutinee choice.`
- **Weakness:** The user cannot tell which qualifier was expected or which choice the compiler resolved as the scrutinee. The generic wording is especially unhelpful in imported or alias-heavy code.
- **Proposed diagnostic:** `Match arm uses qualifier 'Size', but the scrutinee is choice 'Color'. Use a Color variant such as 'Color::Red', or omit the qualifier and write 'Red'.`
- **Test coverage to add:** `tests/cases/match_qualifier_mismatch_context` with local names, an imported alias, and a valid qualified arm to prove the diagnostic does not reject legitimate aliases.

### DIAG-051: Choice payload capture errors omit the expected field and capture count

- **Stage:** AST match-pattern payload validation
- **Current code:** `BST-RULE-0049`
- **Snippets:**
  ```beanstalk
  Result ::
      Ok | value Int |,
      Err | message String |,
  ;

  result = Result::Ok(value = 1)
  if result is:
      Ok(missing) => io.line(missing)
      else => io.line("other")
  ;
  ```
  ```beanstalk
  Result ::
      Ok | value Int |,
      Err | message String |,
  ;

  result = Result::Ok(value = 1)
  if result is:
      Ok() => io.line("empty")
      else => io.line("other")
  ;
  ```
- **Current diagnostics:** `Capture binding does not match payload field name in variant 'Ok'.` and `Too few capture bindings for variant 'Ok'.`
- **Weakness:** Both messages omit the declared field `value`, the actual capture name or the expected versus found count. The user has to inspect the choice declaration to discover the repair.
- **Proposed diagnostics:**
  - `Capture 'missing' does not match field 'value' in variant 'Ok'. Use 'Ok(value)' or 'Ok(value as missing)'.`
  - `Variant 'Ok' has 1 payload field ('value'), but this pattern captures none. Use 'Ok(value)'.`
  Carry field names and expected/found counts as structured facts rather than reconstructing them in prose.
- **Test coverage to add:** `tests/cases/match_payload_capture_name_mismatch` and `tests/cases/match_payload_capture_count`, plus a positive renamed-capture case using `as`.

### DIAG-052: Empty non-`else` match arms are silently accepted

- **Stage:** AST statement match parsing
- **Current code:** no diagnostic
- **Snippet:**
  ```beanstalk
  value = 1
  if value is:
      1 =>
      else => io.line("other")
  ;
  ```
- **Current diagnostic:** No error or warning.
- **Weakness:** The language documents bodyless `else =>` as the explicit no-op arm. A normal pattern arm with no body is accepted and can silently discard the matching case, which is particularly dangerous when the next arm is mistaken for its body.
- **Proposed diagnostic:** `Match arm for '1' has no body. Add a statement after '=>'. A bodyless no-op arm is only allowed for 'else =>'.`
- **Test coverage to add:** `tests/cases/match_non_else_empty_body_rejected`, with a positive `tests/cases/pattern_match_bodyless_else_noop_success` regression check and a value-producing match case that must still require a value body.

### DIAG-053: Function signature ending after `->` is reported as a generic type-annotation error

- **Stage:** Header / AST function signature parsing
- **Current code:** `BST-SYNTAX-0014`
- **Snippet:**
  ```beanstalk
  calculate |value Int| ->
  ```
- **Current diagnostic:** `Expected a type annotation but found newline.`
- **Weakness:** The user is missing a function return type after `->`, not writing an invalid declaration type. The message does not say that `:` is also required or explain the no-return alternative.
- **Proposed diagnostic:** `Function signature is missing a return type after '->'. Add a type and ':' such as '-> Int:', or remove '->' if the function returns no values.`
- **Test coverage to add:** `tests/cases/function_signature_missing_return_type`, covering EOF, newline, and a valid no-return signature without `->`.

### DIAG-054: Declaration with `=` and no right-hand side is misreported as uninitialized

- **Stage:** AST declaration parsing
- **Current code:** `BST-RULE-0031`
- **Snippet:**
  ```beanstalk
  value =
  ```
- **Current diagnostic:** `Uninitialized variable 'value'`
- **Weakness:** The source contains an assignment operator, so the missing expression is the actual error. The current message suggests a declaration-state problem and does not tell the user where to add the value.
- **Proposed diagnostic:** `Expected a value after '=' in declaration 'value'. Write 'value = expression'.`
- **Test coverage to add:** `tests/cases/declaration_missing_initializer`, comparing `name =` with a genuinely malformed bare declaration and asserting the missing-right-hand-expression branch points at `=` or its following location.

### DIAG-055: Collection loop source names the type but offers no valid loop shape

- **Stage:** AST loop-header validation
- **Current code:** `BST-SYNTAX-0029`
- **Snippet:**
  ```beanstalk
  count = 1
  loop count |item|:
      io.line(item)
  ;
  ```
- **Current diagnostic:** `Collection loop source must be a collection. Found 'Int'.`
- **Weakness:** It identifies the mismatch but gives no correction. A user may have intended a range loop, which has different syntax from a collection loop.
- **Proposed diagnostic:** `Collection loop source must be a collection, found 'Int'. Use a collection after 'loop', or use range syntax such as 'loop 0 to count by 1 |i|:'.`
- **Test coverage to add:** `tests/cases/loop_non_collection_source`, asserting the found type and suggestion, with positive collection and range loop cases to protect both accepted shapes.

### DIAG-056: Borrow conflicts name the alias but do not explain how to end or avoid it

- **Stage:** Borrow validation / access conflict rendering
- **Current code:** `BST-BORROW-0002` and `BST-BORROW-0003`
- **Snippet:**
  ```beanstalk
  data ~= "hello"
  first ~= data
  second ~= data
  io.line(first)
  ```
- **Current diagnostic:** `Cannot read 'data' as shared while mutable alias 'first' is still active.`
- **Weakness:** The message identifies the two places and the incompatible access modes, but offers no repair. In dense code, users still have to infer whether to read through the alias, stop using it, or narrow its scope. The same gap exists for a shared alias blocking mutation and for a second mutable alias.
- **Proposed diagnostics:** Keep the stable borrow code and branch the renderer by conflict facts:
  - `Cannot read 'data' while mutable alias 'first' is active. Read through 'first' instead, or finish the mutable borrow before using 'data'.`
  - `Cannot mutably access 'data' while shared alias 'shared' is active. Stop using 'shared' before mutating 'data', or move the mutation into a separate scope.`
  - `Cannot mutably access 'data' because mutable alias 'first' is already active. Reuse 'first', or finish that borrow before creating another one.`
  Preserve the existing generic fallback when no conflicting place is available.
- **Test coverage to add:** `tests/cases/borrow_shared_alias_blocks_mutation`, `tests/cases/borrow_mutable_alias_blocks_shared_read`, and `tests/cases/borrow_duplicate_mutable_alias`. Assert the stable code, both place names, and the repair fragment for each conflict branch. Add a passing case proving that narrowing the alias scope or using the alias itself removes the diagnostic.


---

## Summary and prioritization

### Highest impact

1. **Fix terse renderer fallback** (DIAG-013, DIAG-027). Several diagnostics render empty messages in `--terse` mode even though their descriptors have titles. This breaks CI and tooling.
2. **Convert infrastructure errors to user diagnostics** (DIAG-011, DIAG-012, DIAG-035). Malformed templates, value-producing `if` without else value, and duplicate function parameters currently panic through `BST-INFRA-0001`. These should become stable `CompilerDiagnostic` errors.
3. **Add missing diagnostics for silently accepted invalid code** (DIAG-028, DIAG-031, DIAG-052). Invalid string escapes, multi-return assigned to a single target, and empty non-`else` match arms are accepted without complaint.

### Medium impact

- Improve specificity of context-aware messages: result handling, mutable calls, choice patterns, map keys, catch recovery, statement match arms, operator spacing, and generic function signatures.
- Add "did you mean" hints for type names, qualified namespace members, choice match variants, imported namespace members, and missing grouped imports.
- Split umbrella messages (collection mutator, operator spacing, compound assignment, map literal entries) into more specific variants.

### Low impact / polish

- Wording and tone tweaks for humour/levity where appropriate.
- Better source labels for EOF/unclosed constructs.
- Consistent messages between top-level and function-body contexts.

### Implementation note

When implementing this plan, prefer adding or splitting `CompilerDiagnostic` payload reasons rather than adding ad-hoc strings. Every new diagnostic branch should be accompanied by an integration test case that asserts the stable diagnostic code and, where the message itself is the behavior under test, a fragment of the rendered message. Avoid redundant fixtures that only restate the same umbrella error.

### Suggested audit gates before closing

- [ ] All new diagnostics use `CompilerDiagnostic` with stable codes and source locations.
- [ ] No user-facing mistake routes through `BST-INFRA-0001`.
- [ ] Terse renderer outputs a non-empty message for every diagnostic kind.
- [ ] Integration tests assert diagnostic codes, not exact rendered text, except where text is the explicit feature.
- [ ] `just validate` passes after each batch of changes.
- [ ] Progress matrix updated if any language surface changes (e.g., new rejections or newly accepted forms).
