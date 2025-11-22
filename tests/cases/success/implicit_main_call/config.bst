-- Configuration module with implicit main
database_url ~= ""

export get_database_url || -> String:
    return database_url
;

-- This is config's implicit main - runs when config() is called
io("Loading configuration...")
database_url = "postgresql://localhost:5432/mydb"
io("Configuration loaded")