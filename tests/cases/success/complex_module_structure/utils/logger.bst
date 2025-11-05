-- Logger utility module
export log |message String| -> Void:
    ["[LOG] " + message]
;

export debug |message String| -> Void:
    ["[DEBUG] " + message]
;

-- Logger module initialization
["Logger module initialized"]