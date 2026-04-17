-- Storage utility module
#save |key String, value Int|:
    io([: Saving [key] = [value]])
    -- In a real implementation, this would store to the collection
;

#load |key String| -> Int:
    io([: Loading [key]])
    -- In a real implementation, this would load from the collection
    return 42
;

#init_storage ||:
    io("Storage module initialized")
;
