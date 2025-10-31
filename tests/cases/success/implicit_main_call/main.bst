-- Test calling imported file's implicit main function
#import @config

print("Before config initialization")
config()  -- Call config's implicit main
print("After config initialization")

database_url = config.get_database_url()
print("Database URL: " + database_url)