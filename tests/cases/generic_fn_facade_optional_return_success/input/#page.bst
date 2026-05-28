import @utils {exported_keep}

maybe_name String? = exported_keep("Ana")

if maybe_name is "Ana":
    io("optional generic ok")
else
    io("optional generic failed")
;
