-- Configuration module with explicit initialization API.
#init_config ||:
    io("Loading configuration...")
    io("Configuration loaded")
;

#get_database_url || -> String:
    return "postgresql://localhost:5432/mydb"
;
