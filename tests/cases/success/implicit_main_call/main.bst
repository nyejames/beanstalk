-- Test calling imported file's implicit main function
import @(config)

io("Before config initialization")
config()  -- Call config's implicit main
io("After config initialization")

database_url = config.get_database_url()
io([: Database URL: database_url])