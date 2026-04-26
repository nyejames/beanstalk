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
        case Warning => return "warning"
        case Critical => return "critical"
    ;
;

message = format_level(AlertLevel::Warning)
[:exhaustive_choice_match_returns message=[message]]
