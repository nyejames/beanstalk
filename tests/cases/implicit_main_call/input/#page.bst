-- Test calling imported config entrypoints explicitly.
import @config {init_config, get_database_url}

io("Before config initialization")
init_config()
io("After config initialization")

database_url = get_database_url()
io([: Database URL: [database_url]])
