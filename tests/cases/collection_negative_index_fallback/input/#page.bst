render_negative || -> String:
    values = {10, 20, 30}
    result = values.get(-1) ! 42
    return [:collection_negative_index_fallback result=[result]]
;

[:[render_negative()]]
