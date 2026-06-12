parse |text String| -> Int:
    value Int = cast text catch |err|:
        then err.code
    ;

    return value
;

ok = parse("42")
bad = parse("1_000")

[: runtime string int cast: [ok]/[bad]]
