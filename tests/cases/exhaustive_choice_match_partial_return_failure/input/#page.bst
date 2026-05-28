AlertLevel ::
    Ok,
    Watch,
    Warning,
    Critical,
;

format_level |level AlertLevel| -> String:
    if level is:
        Ok => return "ok"
        Watch => return "watch"
        Warning => io("warning")
        Critical => return "critical"
    ;
;

message = format_level(AlertLevel::Warning)
io(message)
