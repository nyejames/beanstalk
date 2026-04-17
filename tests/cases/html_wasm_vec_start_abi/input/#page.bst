-- Canonical test for the bst_start() -> Vec<String> fragment vec ABI.
-- bst_start() accumulates runtime fragment strings into a vec handle,
-- which the JS bootstrap reads via bst_vec_len / bst_vec_get to hydrate slots.

get_greeting || -> String:
    return "Hello from Wasm"
;

msg = get_greeting()
[: [msg]]
