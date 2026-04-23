#Status :: Ready, Busy;

status Status = Status::Busy
label ~= "unknown"
if status is:
    case Ready => label = "ready";
    case Busy => label = "busy";
;
[:choice_template_control_flow_boundary label=[label]]
