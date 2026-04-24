render_branch_write || -> String:
    values ~= {10, 20, 30}
    if true:
        values.get(1) = 99
    ;
    second = values.get(1) ! 0
    return [:collection_indexed_write_readback_branch second=[second]]
;

[:[render_branch_write()]]
