-- Complex grouped relative-path expansion in a template head.
-- This exercises:
-- - leaf relative entry (intro.md)
-- - nested shared prefix grouping (subfolder { ... })
-- - mixed leaf + branch entries in one grouped block
[@docs {
    intro.md,
    subfolder {
        a.md,
        b.md,
    },
    subfolder/another_folder/c.md,
}]
