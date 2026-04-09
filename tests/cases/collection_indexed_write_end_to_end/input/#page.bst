render_indexed_write || -> String:
    values ~= {10, 20, 30}
    values.get(1) = 99
    first = values.get(0) ! 0
    second = values.get(1) ! 0
    len = values.length()
    return [:collection_indexed_write_end_to_end first=[first] second=[second] len=[len]]
;

[:[render_indexed_write()]]
