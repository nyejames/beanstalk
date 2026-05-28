Box type T = |
    value T,
|

Pair type Left, Right = |
    left Left,
    right Right,
|

number_box Box of Int = Box(42)
text_box Box of String = Box("hello")
pair Pair of String, Int = Pair("count", 3)

io(number_box.value)
io(text_box.value)
io(pair.left)

[: generic struct success]
