import @widget { Counter }

value Counter = Counter(41)
result = value.tick()
io([:result=[result]])
