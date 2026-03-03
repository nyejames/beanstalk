-- Configuration module with implicit main
-- This implicit main runs when config() is called from an importing file

io("Loading configuration...")

io("Configuration loaded")

#get_database_url || -> String:
    return "postgresql://localhost:5432/mydb"
;
