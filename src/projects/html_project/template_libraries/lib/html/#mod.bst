-- Basic HTML components and utility formatters for HTML project builds.
-- Provided by the HTML project builder as a builder source library.

-- Basic tags
# p String = [$html:<p style="[$slot("style")]">[$slot]</p>]
# h1 String = [$html:<h1 style="[$slot("style")]">[$slot]</h1>]
# h2 String = [$html:<h2 style="[$slot("style")]">[$slot]</h2>]
# h3 String = [$html:<h3 style="[$slot("style")]">[$slot]</h3>]
# h4 String = [$html:<h4 style="[$slot("style")]">[$slot]</h4>]
# h5 String = [$html:<h5 style="[$slot("style")]">[$slot]</h5>]
# h6 String = [$html:<h6 style="[$slot("style")]">[$slot]</h6>]

# img String = [$html:<img src="[$slot(1)]" style="[$slot("style")]" alt="[$slot]">]
# small String = [$html:<small style="[$slot("style")]">[$slot]</small>]
# large String = [$html:<large style="[$slot("style")]">[$slot]</large>]

-- Formatting helpers
# underline String = [$html:<u style="[$slot("style")]">[$slot]</u>]
# strikethrough String = [$html:<s style="[$slot("style")]">[$slot]</s>]
# table String = [$html:
  <table style="[$slot("style")]">
    [$slot]
  </table>
]

-- Utility style inserts
# center String = [$insert("style"):text-align: center;]
# right String = [$insert("style"):text-align: right;]
# left String = [$insert("style"):text-align: left;]
# smaller String = [$insert("style"):font-size: smaller;]
# medium String = [$insert("style"):font-size: medium;]
# larger String = [$insert("style"):font-size: larger;]
