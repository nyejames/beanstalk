-- Currently failing to compile,
-- something about specifically about a reference inside a template head, then double template close
-- Is fine if its not a reference (a literal), or if there is a space between the two template closes
-- It panics when checking the token inside src/compiler_frontend/ast/expressions/parse_expression.rs:617
-- Due to no more tokens remaining in the input stream. But there should be a newline / end token it is detecting.
fn || -> String:
    ref = 1
    return [: test[ref]]
;