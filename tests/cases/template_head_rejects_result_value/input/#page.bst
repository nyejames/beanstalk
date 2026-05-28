-- AST rejects Result value in template head expression

value = Int("not_a_number")
[: [value]]
