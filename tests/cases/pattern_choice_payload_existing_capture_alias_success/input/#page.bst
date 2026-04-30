Result ::
    Ok,
    Err |
        message String,
    |,
;

r = Result::Err("bad")

if r is:
    case Ok => io("ok")
    case Err(message as error_message) => io(error_message)
    else => io("else")
;

[:pattern_choice_payload_existing_capture_alias_success]
