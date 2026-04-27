#TaskResult ::
    Success |
        value Int,
        label String,
    |,
    Failure |
        reason String,
        code Int,
    |,
    Pending,
;

#format_task |t TaskResult| -> String:
    if t is:
        case Success(value, label) => return "ok"
        case Failure(reason, code) => return "err"
        case Pending => return "pending"
    ;
;
