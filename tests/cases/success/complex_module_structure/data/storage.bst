-- Storage utility module
storage_data ~= {}

export save |key String, value Int| -> Void:
    ["Saving " + key + " = " + value]
    -- In a real implementation, this would store to the collection
;

export load |key String| -> Int:
    ["Loading " + key]
    -- In a real implementation, this would load from the collection
    return 42
;

-- Storage module initialization
["Storage module initialized"]