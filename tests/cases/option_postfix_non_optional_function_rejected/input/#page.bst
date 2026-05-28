find_name || -> String?:
    return "Ada"
;

display_name || -> String:
    name = find_name()?
    return name
;

[:[display_name()]]
