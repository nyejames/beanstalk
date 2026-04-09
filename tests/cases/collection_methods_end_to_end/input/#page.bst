-- Semantic collection-method coverage surfaced through final artifact output.

render_collection_methods || -> String:
    values ~= {10, 20, 30}
    ~values.push(40)
    ~values.set(0, 99)
    ~values.remove(1)
    first = values.get(0) ! 0
    len = values.length()
    return [:collection_methods_end_to_end first=[first] len=[len]]
;

[:[render_collection_methods()]]
