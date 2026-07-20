-- A map literal consumes an inserted non-copy key; a later use is invalid.
key ~= "Ada"
scores ~{String = Int} = {key = 10}
label = key
