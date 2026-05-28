import @helper { Counter, tick as next }
import @helper { tick as next }

run || -> String:
    c = Counter(41)
    result = c.next()
    return "duplicate_receiver_alias_ok"
;

io(run())
