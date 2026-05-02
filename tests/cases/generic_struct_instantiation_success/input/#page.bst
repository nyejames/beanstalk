Box type T = |
    value T,
|

number_box Box of Int = Box(42)
text_box Box of String = Box("hello")

io(number_box.value)
io(text_box.value)

[: generic struct success]
