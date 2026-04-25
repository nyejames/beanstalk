-- Cross-file constant soft dependency.
-- Tests that imported constant placeholders are visible for deferred resolution.

import @values/later
import @values/base

#total Int = later + 1

[:[total]]
