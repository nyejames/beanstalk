import @test/pkg-a { fallible_text_err }

value = fallible_text_err() catch |err|:
    io(err.message)
    then "fallback"
;

io([:external_fallible_catch_error_fallback value=[value]])
