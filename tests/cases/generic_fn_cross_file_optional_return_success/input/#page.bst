import @./helpers {keep}

maybe_name String? = keep("Ana")

if maybe_name is "Ana":
    io("optional generic ok")
else
    io("optional generic failed")
;
