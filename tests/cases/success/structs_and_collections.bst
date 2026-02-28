-- Test structs and collections in JavaScript backend
-- Validates struct construction, field access, and collection operations

Point = |
    x Int,
    y Int,
|

Person = |
    name String,
    age Int,
|

main ||:
    -- Test struct construction
    p1 = Point(10, 20)
    io([: Point: (p1.x, p1.y)])
    
    -- Test struct with strings
    person = Person("Alice", 30)
    io([: Person: person.name, age person.age])
    
    -- Test field assignment
    p2 = Point(0, 0)
    p2.x = 5
    p2.y = 10
    io([: Updated point: (p2.x, p2.y)])
    
    -- Test empty collection
    empty = {}
    io([: Empty collection length: empty.length()])
    
    -- Test collection with elements
    numbers = {1, 2, 3, 4, 5}
    io([: Numbers length: numbers.length()])
    io([: First element: numbers.get(0)])
    io([: Last element: numbers.get(4)])
    
    -- Test collection of strings
    names = {"Alice", "Bob", "Charlie"}
    io([: Names: names.get(0), names.get(1), names.get(2)])
    
    -- Test nested collections
    matrix = {{1, 2}, {3, 4}}
    io([: Matrix[0][0] = matrix.get(0).get(0)])
    io([: Matrix[1][1] = matrix.get(1).get(1)])
    
    io("Struct and collection tests passed")
;
