-- Test import syntax and entry point execution
#import @utils/math
#import @utils/string

result = math.add(5, 3)
formatted = string.format_number(result)
io([: Result: formatted])