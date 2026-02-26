-- Configuration module with implicit main
-- This is config's implicit main - runs when config.start() is called

io("Loading configuration...")

-- This is part of the string the start function will return
[: "postgresql://localhost:5432/mydb"]

io("Configuration loaded")