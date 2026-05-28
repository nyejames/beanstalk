-- Prove write-through alias semantics: struct field mutation is observable through
-- an alias created after the write, demonstrating __bs_write, __bs_assign_borrow, and __bs_read.

Counter = |
    value Int,
|

render_write_through || -> String:
    c ~= Counter(10)
    c.value = c.value + 5  -- triggers __bs_write on the computed field place
    alias = c              -- triggers __bs_assign_borrow
    -- Both reads route through __bs_read
    return [:write_through value=[c.value] alias_value=[alias.value]]
;

[:[render_write_through()]]
