import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

page_title #= "Core Time"
page_description #= "Duration, TimeMark, and Timestamp helpers in @core/time."
page_head #= theme_head

#[navbar]
#[title, center: CORE TIME]

#[section, $markdown:
    [codesnippet:@core/time] separates elapsed time from wall-clock time.

    - [codesnippet:Duration] is for elapsed amounts.
    - [codesnippet:TimeMark] is for measuring elapsed time and frame deltas.
    - [codesnippet:Timestamp] is for real-world UTC instants.

    [codeblock, $code("bst"):
        import @core/time {
            Duration,
            TimeMark,
            mark_now,
            duration_between,
        }
    ]
]

#[section, $markdown:
    ## Measuring elapsed time

    Use [codesnippet:TimeMark] when you need a monotonic measurement point.
    Use [codesnippet:Duration] for the elapsed amount.

    [codeblock, $code("bst"):
        previous ~= mark_now()

        current = mark_now()
        delta = duration_between(previous, current)
        previous = current

        seconds = delta.as_seconds()
    ]

    This shape works for frame-delta style code without requiring a scheduler or animation package.
]

#[section, $markdown:
    ## UTC timestamps

    Use [codesnippet:Timestamp] for real-world UTC instants that may be logged, stored, or exchanged with external systems.

    [codeblock, $code("bst"):
        import @core/time {
            Timestamp,
            timestamp_from_iso_string,
            timestamp_from_unix_milliseconds,
        }

        parse_created || -> Timestamp, Error!:
            return timestamp_from_iso_string("1970-01-01T00:00:00.000Z")!
        ;

        created = parse_created() catch:
            then timestamp_from_unix_milliseconds(0.0)
        ;

        io(created.to_iso_string())
    ]

    Invalid ISO strings are ordinary fallible calls:

    [codeblock, $code("bst"):
        import @core/time {timestamp_from_iso_string, timestamp_from_unix_milliseconds}

        created = timestamp_from_iso_string("not-a-date") catch:
            then timestamp_from_unix_milliseconds(0.0)
        ;
    ]
]

#[section, $markdown:
    ## API surface

    [styled_table:
        [header: [: Name] [: Meaning] ]

        [data: [: [codesnippet:mark_now()] ] [: Current monotonic [codesnippet:TimeMark].] ]
        [data: [: [codesnippet:elapsed_since(start)] ] [: [codesnippet:Duration] from a mark to now.] ]
        [data: [: [codesnippet:duration_between(start, end)] ] [: [codesnippet:Duration] between two marks.] ]
        [data: [: [codesnippet:timestamp_now()] ] [: Current UTC wall-clock [codesnippet:Timestamp].] ]
        [data: [: [codesnippet:duration_from_seconds(seconds)] ] [: Build a [codesnippet:Duration].] ]
        [data: [: [codesnippet:duration_from_milliseconds(milliseconds)] ] [: Build a [codesnippet:Duration].] ]
        [data: [: [codesnippet:timestamp_from_unix_seconds(seconds)] ] [: Build a [codesnippet:Timestamp].] ]
        [data: [: [codesnippet:timestamp_from_unix_milliseconds(milliseconds)] ] [: Build a [codesnippet:Timestamp].] ]
        [data: [: [codesnippet:timestamp_from_iso_string(text)] ] [: Parse an ISO timestamp as [codesnippet:Timestamp, Error!].] ]
        [data: [: [codesnippet:duration.as_seconds()] ] [: Convert a [codesnippet:Duration] to seconds.] ]
        [data: [: [codesnippet:duration.as_milliseconds()] ] [: Convert a [codesnippet:Duration] to milliseconds.] ]
        [data: [: [codesnippet:duration.is_negative()] ] [: Check whether a [codesnippet:Duration] is below zero.] ]
        [data: [: [codesnippet:duration.abs()] ] [: Absolute [codesnippet:Duration].] ]
        [data: [: [codesnippet:duration.clamp(min, max)] ] [: Bound a [codesnippet:Duration] to a range.] ]
        [data: [: [codesnippet:timestamp.unix_seconds()] ] [: Convert a [codesnippet:Timestamp] to Unix seconds.] ]
        [data: [: [codesnippet:timestamp.unix_milliseconds()] ] [: Convert a [codesnippet:Timestamp] to Unix milliseconds.] ]
        [data: [: [codesnippet:timestamp.to_iso_string()] ] [: Format a [codesnippet:Timestamp] as UTC ISO text.] ]
    ]
]

#[section, $markdown:
    ## What is deferred?

    The first implementation intentionally does not include:

    - calendar date types
    - time zones or local-time conversion
    - locale-aware formatting
    - timers, sleep, intervals, or animation callbacks
    - Wasm or non-JS time lowerings

    Browser animation-frame integration belongs in a future web-specific package rather than [codesnippet:@core/time].
]
