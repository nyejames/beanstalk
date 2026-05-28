empty_values type T || -> {T}:
    return {}
;

numbers {Int} = empty_values()

io(numbers.length())
