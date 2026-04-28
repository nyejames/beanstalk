Pair ::
    Pair |
        left Int,
        right Int,
    |,
;

p = Pair::Pair(1, 2)

if p is:
    case Pair(left as value, right as value) => io("ok")
;
