-- Complex multi-file module test with various features
import @(utils/logger/log)
import @(utils/calculator/compute)
import @(data/storage/{save, load})

io("Starting complex module test")

-- Test host function with imported module interaction
log("Initializing calculator")
result = compute(10, 5)
log("Calculation complete")

-- Test storage operations
save("result", result)
saved_value = load("result")
io("Saved and loaded value: " + saved_value)

io("Complex module test complete")
