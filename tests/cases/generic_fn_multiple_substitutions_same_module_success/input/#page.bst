import @./helpers {identity}

int_value = identity(42)
string_value = identity("hello")
float_value = identity(3.14)

io([: [int_value] [string_value] [float_value]])
