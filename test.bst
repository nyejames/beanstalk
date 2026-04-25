-- Should not compile
bad Int =  6

// 

5

func |a Int, b Int| -> Int:
    return a + b
;

good Int = func(10, 20)

[good]