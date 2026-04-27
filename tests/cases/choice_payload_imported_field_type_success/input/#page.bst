import @types/Label

#Response :: Err | message Label |, Success;

use_choice |r Response| -> String:
    if r is:
        case Success => return "ok"
        case Response::Err(message) => return "err"
    ;
;

label = use_choice(Response::Success)
[:choice_payload_imported_field_type_success label=[label]]
