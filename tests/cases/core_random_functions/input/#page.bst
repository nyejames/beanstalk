import @core/random {random_float, random_int}

value = random_float()
count = random_int(1, 3)

io([:core_random value=[value] count=[count]])
