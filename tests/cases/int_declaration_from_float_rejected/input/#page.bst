-- Validates that Float cannot be implicitly narrowed to Int.
-- Coercion only goes Int → Float, never Float → Int.

result Int = 1.5

io("should not reach here")
