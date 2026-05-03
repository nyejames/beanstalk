-- Cross-file constant dependency.
-- Tests that the AST constant graph orders imported constants before dependents.

import @values/later
import @values/base

#total Int = later + 1

[:[total]]
