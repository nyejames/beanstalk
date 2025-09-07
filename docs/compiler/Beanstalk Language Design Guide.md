---
inclusion: always
---

# Beanstalk Language Design Guide

Beanstalk is a programming language and build system with minimal syntax and a simple type system.

You can think of the language at a high level as being similar to Go (fast compile times, very minimal and simple), but with a Rust style memory management (instead of a GC) and a unique modern syntax with very powerful templates.

## Language Syntax Rules

**Critical syntax requirements when generating or parsing Beanstalk code:**

- **Scope delimiters**: `:` opens scope, `;` closes scope (NOT statement terminators)
- **Collections**: `{}` for arrays/collections, `[]` ONLY for templates
- **Operators**: Keywords only (`is`, `not`, `and`, `or`) - NEVER use `==`, `!=`, `!`
- **Mutability**: `~` prefix for mutable types (`~Int`, `~String`)
- **Comments**: `--` for single-line comments
- **Indentation**: Always 4 spaces (enforce in all generated code)

**Naming conventions (strictly enforce):**
- Types/Objects: `Upper_Snake_Case`
- Variables/functions: `regular_snake_case`
## Core Syntax Patterns

**Variable declarations:**
```beanstalk
-- Mutable variables use ~=
int_value ~= 0
string_value ~= "text"
collection ~= {}

-- Immutable variables use =
constant_value = 42
immutable_collection = {}
```

**Function definitions:**
```beanstalk
-- Basic function pattern
function_name |param Type| -> ReturnType:
    -- 4-space indentation
    return value
;

-- Error handling pattern
risky_function || -> String, Error!:
    return other_function() !err:
        return "", err
    ;
;
```

**Control flow patterns:**
```beanstalk
-- Conditional (use 'is', never ==)
if value is not 0:
    -- code
else
    -- code
;

-- Pattern matching (always exhaustive)
if value is:
    0: print("zero")
    < 0: print("negative")
    else: print("other")
;
```
## Template System

**Templates use `[]` exclusively** - never confuse with collections `{}`.

**Template structure:**
- Head and body separated by `:`
- Variable capture from surrounding scope
- Runtime ID assignment with `@` symbol
- Built-in library integration with `#` prefix

**Template patterns:**
```beanstalk
-- Basic template
[element: content]

-- With style/library
[Section #markdown: content]

-- With ID for runtime access
[section @my_id: content]

-- Control flow in templates
[if condition: content]
[for item in collection: [item]]
```

**Memory model:**
- Borrow checker without explicit lifetimes. No unsafe.
- Reference passing by default, `~` for mutable
- Move semantics determined by compiler analysis
- ARC fallback for complex scenarios

