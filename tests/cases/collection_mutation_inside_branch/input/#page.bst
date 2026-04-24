render_branch_mutation || -> String:
    values ~= {1, 2, 3}
    condition = true
    if condition:
        ~values.push(4)
    else
        ~values.remove(0)
    ;
    ~values.set(0, 10)
    first = values.get(0) ! 0
    len = values.length()
    return [:collection_mutation_inside_branch first=[first] len=[len]]
;

[:[render_branch_mutation()]]
