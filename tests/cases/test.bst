-- Test host function call
print("Hello from Beanstalk!")

-- Test template assignment to variables
template_result = ["hello world"]

-- Test template with head variables
name = "Beanstalk"
greeting = [name: " is awesome!"]

-- Test template with multiple parts
number = 42
message = [number, " is the answer"]

-- Test template with different primitive types
pi = 3.14159
is_cool = true
complex_message = [name, " has ", number, " features and pi is ", pi, ". Is it cool? ", is_cool]