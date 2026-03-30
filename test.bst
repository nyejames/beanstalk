
Person = |
    name String,
    age Int,
|

-- Create a new instance of the type
person ~= Person("Alice", 30)

-- Access fields using dot notation
io(person.name) -- "Alice"
io(person.age)  -- 30

-- Defining a plex, then defining a method for it
Vector2 = |
    x Float = 0,
    y Float = 0,
|

reset |this ~Vector2|:
    this.x = 0
    this.y = 0
;

vec = Vector2(12, 87)
vec.reset()

io(vec.x) -- 0

