display_name |maybe_name String?| -> String:
    name = if maybe_name is |name| then name else "guest"
    return name
;

maybe_missing String? = none

present_name = display_name("Ada")
missing_name = display_name(maybe_missing)

io("option_value_if_inline_unwrap")
io([:present [present_name]])
io([:missing [missing_name]])
