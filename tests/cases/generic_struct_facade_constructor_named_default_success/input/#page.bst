-- Generic struct constructor through a facade import, using named arguments
-- and a defaulted field that is omitted at the call site.

import @utils { Wrapper }

wrapper Wrapper of Int = Wrapper(value = 42)

io(wrapper.value)
io(wrapper.label)
