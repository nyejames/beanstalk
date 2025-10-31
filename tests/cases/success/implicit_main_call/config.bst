-- Configuration module with implicit main
database_url ~= ""

export get_database_url || -> String:
    return database_url
;

-- This is config's implicit main - runs when config() is called
print("Loading configuration...")
database_url = "postgresql://localhost:5432/mydb"
print("Configuration loaded")