-- A value-producing branch that falls through without producing or terminating.
value = if true:
    io("side effect")
else
    then 0
;
