-- Semantic collection-method coverage surfaced through final artifact output.

render_collection_methods || -> String:
    values ~= {10, 20, 30}
    ~values.push(40)
    ~values.set(0, 99) catch:
    ;
    removed = ~values.remove(1) catch:
        then 0
    ;
    first = values.get(0) catch:
        then 0
    ;
    len = values.length()
    return [:collection_methods_end_to_end first=[first] len=[len]]
;

[:[render_collection_methods()]]
