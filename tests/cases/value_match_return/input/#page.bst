Status ::
    Ready,
    Waiting,
;

label_for |status Status| -> String:
    return if status is:
        Ready => then "ready"
        else => then "other"
    ;
;

io(label_for(Status::Waiting))
