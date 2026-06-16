Inner ::
    A | x Int |,
    B,
;

Outer ::
    Wrap | inner Inner |,
    Empty,
;

a = Outer::Wrap(Inner::A(1))
b = Outer::Wrap(Inner::A(2))

if a is b:
    io.line("same")
else
    io.line("different")
;
