-- Validates that Bool cannot be implicitly promoted to Float.
-- Only Int → Float is supported as a contextual coercion.

result Float = true

io("should not reach here")
