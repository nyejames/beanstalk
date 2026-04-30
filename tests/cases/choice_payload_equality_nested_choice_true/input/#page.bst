#Inner ::
    A | x Int |,
    B,
;

#Outer ::
    Wrap | inner Inner |,
    Empty,
;

a = Outer::Wrap(Inner::A(1))
b = Outer::Wrap(Inner::A(1))

if a is b:
    io("same")
else
    io("different")
;
