empty_values type T || -> {T}:
    return {}
;

numbers {Int} = empty_values()
wrapped_numbers {Int} = (empty_values())

io(numbers.length())
io(wrapped_numbers.length())
