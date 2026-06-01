RESETTABLE must:
    reset |~This|
;

Counter = |
    value Int,
|

reset |this ~Counter|:
    this.value = 0
;

Counter must RESETTABLE

use_dynamic |value ~RESETTABLE|:
    value.reset()
;

[:unreachable]
