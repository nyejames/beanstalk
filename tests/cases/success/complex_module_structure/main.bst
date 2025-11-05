-- Complex multi-file module test with various features
#import @utils/logger
#import @utils/calculator
#import @data/storage

print("Starting complex module test")

-- Test host function with imported module interaction
logger.log("Initializing calculator")
result = calculator.compute(10, 5)
logger.log("Calculation complete")

-- Test storage operations
storage.save("result", result)
saved_value = storage.load("result")
print("Saved and loaded value: " + saved_value)

print("Complex module test complete")