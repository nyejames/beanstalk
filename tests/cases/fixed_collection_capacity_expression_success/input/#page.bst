render_capacity_expr || -> String:
    base #Int = 2
    items ~{base + 2 Int} = {}
    ~items.push(7) catch:
    ;
    first = items.get(0) catch:
        then 0
    ;
    len = items.length()
    return [:fixed_collection_capacity_expression_success length=[len] value=[first]]
;

[:[render_capacity_expr()]]
