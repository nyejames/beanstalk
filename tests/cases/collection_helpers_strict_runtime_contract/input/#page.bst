-- Success-path coverage for all collection helpers with strict runtime contract.

render_strict_contract || -> String:
    values ~= {10, 20, 30}
    ~values.push(40)
    ~values.remove(1)
    first = values.get(0) ! 0
    len = values.length()
    return [:strict_contract first=[first] len=[len]]
;

[:[render_strict_contract()]]
