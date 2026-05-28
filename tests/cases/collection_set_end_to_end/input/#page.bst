render_collection_set || -> String:
    values ~= {10, 20, 30}
    ~values.set(1, 99) catch:
    ;
    first = values.get(0) catch:
        then 0
    ;
    second = values.get(1) catch:
        then 0
    ;
    len = values.length()
    return [:collection_set_end_to_end first=[first] second=[second] len=[len]]
;

[:[render_collection_set()]]
