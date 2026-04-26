#AlertLevel ::
    Ok,
    Watch,
    Warning,
    Critical,
;

format_level |level AlertLevel| -> String:
    if level is:
        case Ok => return "ok"
        case Watch => return "watch"
        case Warning => io("warning")
        case Critical => return "critical"
    ;
;

message = format_level(AlertLevel::Warning)
io(message)
