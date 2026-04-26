AlertLevel ::
    Ok,
    Watch,
    Warning,
    Critical,
;

SensorKind ::
    Temperature,
    Humidity,
    Pressure,
;

Reading = |
    kind SensorKind,
    value Float,
    sensor_name String,
|

DayReport = |
    day String,
    readings {Reading},
|

format_level |level AlertLevel| -> String:
    if level is:
        case Ok => return "ok"
        case Watch => return "watch"
        case Warning => return "warning"
        case Critical => return "critical"
    ;
;

kind_name |kind SensorKind| -> String:
    if kind is:
        case Temperature => return "temperature"
        case Humidity => return "humidity"
        case Pressure => return "pressure"
    ;
;

score_temperature |value Float| -> AlertLevel:
    if value is:
        case < 0.0 => return AlertLevel::Warning
        case >= 80.0 => return AlertLevel::Critical
        case >= 45.0 => return AlertLevel::Warning
        case >= 35.0 => return AlertLevel::Watch
        else => return AlertLevel::Ok
    ;
;

score_humidity |value Float| -> AlertLevel:
    if value is:
        case >= 90.0 => return AlertLevel::Warning
        case >= 75.0 => return AlertLevel::Watch
        case < 15.0 => return AlertLevel::Watch
        else => return AlertLevel::Ok
    ;
;

score_pressure |value Float| -> AlertLevel:
    if value is:
        case < 950.0 => return AlertLevel::Critical
        case < 980.0 => return AlertLevel::Warning
        case > 1040.0 => return AlertLevel::Warning
        else => return AlertLevel::Ok
    ;
;

score_reading |reading Reading| -> AlertLevel:
    if reading.kind is:
        case Temperature => return score_temperature(reading.value)
        case Humidity => return score_humidity(reading.value)
        case Pressure => return score_pressure(reading.value)
    ;
;

is_serious |level AlertLevel| -> Bool:
    if level is:
        case Warning => return true
        case Critical => return true
        else => return false
    ;
;

render_reading |reading Reading, level AlertLevel, day String, index Int| -> String:
    kind = kind_name(reading.kind)
    label = format_level(level)

    return [:
        <li class="[label]">
            <strong>[day] #[index]</strong>
            [reading.sensor_name] / [kind] = [reading.value]
            <em>[label]</em>
        </li>
    ]
;

render_day_summary |day DayReport| -> String:
    ok_count ~= 0
    watch_count ~= 0
    warning_count ~= 0
    critical_count ~= 0
    serious_items ~= [:]

    loop day.readings |reading, index|:
        level = score_reading(reading)

        if level is:
            case Ok => ok_count = ok_count + 1
            case Watch => watch_count = watch_count + 1
            case Warning =>
                warning_count = warning_count + 1
                serious_items = [:
                    [serious_items]
                    [render_reading(reading, level, day.day, index)]
                ]
            case Critical =>
                critical_count = critical_count + 1
                serious_items = [:
                    [serious_items]
                    [render_reading(reading, level, day.day, index)]
                ]
        ;

        if is_serious(level):
            io([: serious reading found in [day.day] ])
        ;
    ;

    return [:
        <section class="day">
            <h2>[day.day]</h2>

            <p>
                ok: [ok_count],
                watch: [watch_count],
                warning: [warning_count],
                critical: [critical_count]
            </p>

            <ul>
                [serious_items]
            </ul>
        </section>
    ]
;

render_dashboard |reports {DayReport}| -> String:
    page ~= [:]
    total_days ~= 0
    total_serious ~= 0

    loop reports |report|:
        total_days = total_days + 1
        day_has_serious ~= false

        loop report.readings |reading|:
            level = score_reading(reading)

            if is_serious(level):
                total_serious = total_serious + 1
                day_has_serious = true
            ;
        ;

        if day_has_serious is true:
            page = [:
                [page]
                [render_day_summary(report)]
            ]
        else
            page = [:
                [page]
                <section class="day quiet">
                    <h2>[report.day]</h2>
                    <p>No serious readings.</p>
                </section>
            ]
        ;
    ;

    severity_label ~= "normal"

    if total_serious is:
        case 0 => severity_label = "normal"
        case <= 2 => severity_label = "watch"
        case <= 5 => severity_label = "warning"
        else => severity_label = "critical"
    ;

    return [:
        <main class="[severity_label]">
            <h1>Sensor dashboard</h1>
            <p>[total_days] days scanned.</p>
            <p>[total_serious] serious readings found.</p>

            [page]
        </main>
    ]
;

monday_readings ~{Reading} = {}
~monday_readings.push(Reading(SensorKind::Temperature, 21.0, "greenhouse-a"))
~monday_readings.push(Reading(SensorKind::Humidity, 91.0, "greenhouse-a"))
~monday_readings.push(Reading(SensorKind::Pressure, 1012.0, "weather-station"))

tuesday_readings ~{Reading} = {}
~tuesday_readings.push(Reading(SensorKind::Temperature, 83.0, "greenhouse-b"))
~tuesday_readings.push(Reading(SensorKind::Humidity, 50.0, "greenhouse-b"))
~tuesday_readings.push(Reading(SensorKind::Pressure, 945.0, "weather-station"))

reports ~{DayReport} = {}
~reports.push(DayReport("Monday", monday_readings))
~reports.push(DayReport("Tuesday", tuesday_readings))

[:
    [render_dashboard(reports)]
]