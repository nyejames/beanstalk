-- Test calling imported file's implicit main function
#import @config

["Before config initialization"]
config()  -- Call config's implicit main
["After config initialization"]

database_url = config.get_database_url()
["Database URL: " + database_url]