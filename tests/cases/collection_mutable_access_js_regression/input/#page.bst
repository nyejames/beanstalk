render_js_regression || -> String:
    values ~= {1}
    ~values.push(2)
    ~values.set(0, 9)
    values.get(1) = 99
    ~values.remove(0)
    first = values.get(0) ! 0
    len = values.length()
    return [:collection_mutable_access_js_regression first=[first] len=[len]]
;

[:[render_js_regression()]]
