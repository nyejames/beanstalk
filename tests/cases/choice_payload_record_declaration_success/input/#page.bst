#Response :: Err | message String |, Success;

use_choice |r Response| -> String:
    if r is:
        case Success => return "ok"
        case Response::Err => return "err"
    ;
;

label = use_choice(Response::Success)
[:choice_payload_record_declaration_success label=[label]]
