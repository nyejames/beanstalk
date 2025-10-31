-- Logger utility module
export log |message String| -> Void:
    print("[LOG] " + message)
;

export debug |message String| -> Void:
    print("[DEBUG] " + message)
;

-- Logger module initialization
print("Logger module initialized")