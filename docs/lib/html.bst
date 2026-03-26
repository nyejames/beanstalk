-- Basic HTML components and utility formatters for docs/theme code.
-- The HTML project builder now owns the outer document shell and baseline CSS reset.

Format = |
    -- Basic formatting helpers
    underline String = [$html:<u style="[$slot("style")]">[$slot]</u>],
    strikethrough String = [$html:<s style="[$slot("style")]">[$slot]</s>],
    table String = [$html:
      <table style="[$slot("style")]">
        [$slot]
      </table>
    ],
|

# format = Format()

-- Inline utility styles
# center String = [$insert("style"):text-align: center;]
# right String = [$insert("style"):text-align: right;]
# left String = [$insert("style"):text-align: left;]

-- Basic HTML components
# img String = [$html:<img src="[$slot(1)]" style="[$slot("style")]" alt="[$slot]">]
