values ~= {10, 20, 30}
~values.set(1, 99)
len = values.length()
picked = values.get(1) ! 0

[:html_wasm_collection_runtime_coverage len=[len] picked=[picked]]
