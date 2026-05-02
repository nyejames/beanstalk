-- pattern-stress.bst
-- Pattern matching stress test:
-- - Exhaustive matches on choice types
-- - Guard expressions
-- - Payload capture patterns
-- - Relational patterns for scalars
-- - Nested match expressions
-- - Large match arms (20+ cases)

-- Choice types for pattern matching
Status ::
    Pending,
    Running,
    Paused,
    Cancelled,
    Completed,
    Failed,
;

Priority ::
    Low,
    Medium,
    High,
    Critical,
    Urgent,
;

Color ::
    Red,
    Green,
    Blue,
    Yellow,
    Magenta,
    Cyan,
    White,
    Black,
;

-- Choices with payloads
Result ::
    Ok,
    Err | message String |,
;

Optional ::
    None,
    Some | value Int |,
;

Message ::
    Text | content String |,
    Number | value Int |,
    Flag | active Bool |,
    Pair | first Int, second Int |,
    Triple | a Int, b Int, c Int |,
;

HttpResponse ::
    Success | code Int, body String |,
    Redirect | url String |,
    ClientError | code Int, message String |,
    ServerError | code Int, message String |,
;

Event ::
    Click | x Int, y Int |,
    KeyPress | key String, ctrl Bool, shift Bool, alt Bool |,
    Scroll | delta Float |,
    Resize | width Int, height Int |,
;

-- Exhaustive match on choice without else
handle_status |status Status| -> String:
    if status is:
        case Pending => return "Status is pending"
        case Running => return "Status is running"
        case Paused => return "Status is paused"
        case Cancelled => return "Status is cancelled"
        case Completed => return "Status is completed"
        case Failed => return "Status is failed"
    ;
;

handle_priority |priority Priority| -> Int:
    if priority is:
        case Low => return 1
        case Medium => return 2
        case High => return 3
        case Critical => return 4
        case Urgent => return 5
    ;
;

handle_color |color Color| -> String:
    if color is:
        case Red => return "#ff0000"
        case Green => return "#00ff00"
        case Blue => return "#0000ff"
        case Yellow => return "#ffff00"
        case Magenta => return "#ff00ff"
        case Cyan => return "#00ffff"
        case White => return "#ffffff"
        case Black => return "#000000"
    ;
;

-- Match with else fallback
handle_status_else |status Status| -> String:
    if status is:
        case Running => return "active"
        case Completed => return "done"
        case Failed => return "error"
        else => return "other"
    ;
;

-- Guard expressions
classify_number |value Int| -> String:
    if value is:
        case 0 => return "zero"
        case 1 => return "one"
        case 2 => return "two"
        case < 0 => return "negative"
        case < 10 => return "small"
        case < 100 => return "medium"
        case < 1000 => return "large"
        else => return "huge"
    ;
;

classify_with_guards |value Int, enabled Bool| -> String:
    if value is:
        case 0 if enabled => return "zero-enabled"
        case 0 => return "zero-disabled"
        case 1 if enabled => return "one-enabled"
        case 1 => return "one-disabled"
        case < 10 if enabled => return "small-enabled"
        case < 10 => return "small-disabled"
        else => return "other"
    ;
;

check_range |value Int| -> String:
    if value is:
        case < 0 => return "below-zero"
        case 0 => return "zero"
        case <= 10 => return "one-to-ten"
        case <= 50 => return "eleven-to-fifty"
        case <= 100 => return "fifty-one-to-hundred"
        case > 100 => return "above-hundred"
        else => return "impossible"
    ;
;

-- Payload capture patterns
handle_result |result Result| -> String:
    if result is:
        case Ok => return "Success"
        case Err(message) => return [: Error: [message]]
    ;
;

handle_optional |opt Optional| -> String:
    if opt is:
        case None => return "No value"
        case Some(value) => return [: Value: [value]]
    ;
;

handle_message |msg Message| -> String:
    if msg is:
        case Text(content) => return [: Text: [content]]
        case Number(value) => return [: Number: [value]]
        case Flag(active) => return [: Flag: [active]]
        case Pair(first, second) => return [: Pair: ([first], [second])]
        case Triple(a, b, c) => return [: Triple: ([a], [b], [c])]
    ;
;

handle_http |response HttpResponse| -> String:
    if response is:
        case Success(code, body) => return [: Success [code]: [body]]
        case Redirect(url) => return [: Redirect to [url]]
        case ClientError(code, message) => return [: Client error [code]: [message]]
        case ServerError(code, message) => return [: Server error [code]: [message]]
    ;
;

handle_event |event Event| -> String:
    if event is:
        case Click(x, y) => return [: Click at ([x], [y])]
        case KeyPress(key, ctrl, shift, alt) => return [: Key: [key] ctrl=[ctrl] shift=[shift] alt=[alt]]
        case Scroll(delta) => return [: Scroll: [delta]]
        case Resize(width, height) => return [: Resize: [width]x[height]]
    ;
;

-- Relational patterns
compare_to_ten |value Int| -> String:
    if value is:
        case < 10 => return "less than ten"
        case 10 => return "exactly ten"
        case > 10 => return "greater than ten"
        else => return "impossible"
    ;
;

grade_score |score Int| -> String:
    if score is:
        case < 0 => return "invalid"
        case < 60 => return "F"
        case < 70 => return "D"
        case < 80 => return "C"
        case < 90 => return "B"
        case <= 100 => return "A"
        case > 100 => return "invalid"
        else => return "unknown"
    ;
;

categorize_string |text String| -> String:
    if text is:
        case < "a" => return "before-a"
        case < "m" => return "a-to-l"
        case < "z" => return "m-to-y"
        case "z" => return "exactly-z"
        else => return "z-or-after"
    ;
;

-- Nested match expressions
outer = 10
inner = 5

nested_match || -> String:
    if outer is:
        case < 5 => 
            if inner is:
                case < 5 => return "outer-small inner-small"
                case 5 => return "outer-small inner-five"
                else => return "outer-small inner-large"
            ;
        case 10 =>
            if inner is:
                case < 5 => return "outer-ten inner-small"
                case 5 => return "outer-ten inner-five"
                else => return "outer-ten inner-large"
            ;
        else =>
            if inner is:
                case < 5 => return "outer-other inner-small"
                else => return "outer-other inner-other"
            ;
    ;
;

-- Large match (20+ arms)
large_match |value Int| -> String:
    if value is:
        case 0 => return "zero"
        case 1 => return "one"
        case 2 => return "two"
        case 3 => return "three"
        case 4 => return "four"
        case 5 => return "five"
        case 6 => return "six"
        case 7 => return "seven"
        case 8 => return "eight"
        case 9 => return "nine"
        case 10 => return "ten"
        case 11 => return "eleven"
        case 12 => return "twelve"
        case 13 => return "thirteen"
        case 14 => return "fourteen"
        case 15 => return "fifteen"
        case 16 => return "sixteen"
        case 17 => return "seventeen"
        case 18 => return "eighteen"
        case 19 => return "nineteen"
        case 20 => return "twenty"
        else => return "other"
    ;
;

-- Payload capture with guards
guarded_payload |opt Optional| -> String:
    if opt is:
        case None => return "no value"
        case Some(value) if value < 0 => return "negative value"
        case Some(value) if value is 0 => return "zero value"
        case Some(value) if value < 100 => return "small positive"
        case Some(value) => return "large positive"
    ;
;

guarded_http |response HttpResponse| -> String:
    if response is:
        case Success(code, body) if code < 200 => return "info"
        case Success(code, body) if code < 300 => return "success"
        case Redirect(url) => return "redirect"
        case ClientError(code, message) if code is 400 => return "bad request"
        case ClientError(code, message) if code is 404 => return "not found"
        case ClientError(code, message) => return "client error"
        case ServerError(code, message) if code is 500 => return "internal error"
        case ServerError(code, message) => return "server error"
    ;
;

-- Mixed patterns in sequence
process_all |status Status, priority Priority, result Result| -> String:
    status_str = handle_status(status)
    priority_num = handle_priority(priority)
    result_str = handle_result(result)
    return [: Status: [status_str] / Priority: [priority_num] / Result: [result_str]]
;

-- Capture pattern for whole value
capture_whole |value Int| -> String:
    if value is:
        case captured if captured < 0 => return [: Negative: [captured]]
        case captured => return [: Non-negative: [captured]]
        else => return "unreachable"
    ;
;

-- Multiple choice match sequence
match_sequence || -> String:
    result ~= ""
    
    statuses = {Status::Pending, Status::Running, Status::Completed, Status::Failed}
    
    loop statuses |s|:
        str = handle_status(s)
        result = [: [result][str]; ]
    ;
    
    return result
;

-- Stress test entry
run_pattern_stress ||:
    io("Pattern matching stress test starting")
    
    -- Exhaustive matches
    io(handle_status(Status::Running))
    io(handle_status(Status::Failed))
    io(handle_priority(Priority::Critical))
    io(handle_color(Color::Magenta))
    
    -- Matches with else
    io(handle_status_else(Status::Pending))
    
    -- Guards
    io(classify_number(0))
    io(classify_number(-5))
    io(classify_number(50))
    io(classify_number(500))
    io(classify_with_guards(0, true))
    io(classify_with_guards(0, false))
    io(check_range(25))
    
    -- Payload captures
    io(handle_result(Result::Ok))
    io(handle_result(Result::Err("something went wrong")))
    io(handle_optional(Optional::None))
    io(handle_optional(Optional::Some(42)))
    io(handle_message(Message::Text("hello")))
    io(handle_message(Message::Pair(10, 20)))
    io(handle_http(HttpResponse::Success(200, "OK")))
    io(handle_http(HttpResponse::ClientError(404, "Not Found")))
    io(handle_event(Event::Click(100, 200)))
    io(handle_event(Event::KeyPress("Enter", true, false, false)))
    
    -- Relational patterns
    io(compare_to_ten(5))
    io(compare_to_ten(10))
    io(compare_to_ten(15))
    io(grade_score(85))
    io(categorize_string("hello"))
    
    -- Nested match
    io(nested_match())
    
    -- Large match
    io(large_match(15))
    io(large_match(100))
    
    -- Guarded payloads
    io(guarded_payload(Optional::Some(-10)))
    io(guarded_payload(Optional::Some(50)))
    io(guarded_http(HttpResponse::Success(200, "OK")))
    io(guarded_http(HttpResponse::ClientError(404, "Not Found")))
    
    -- Capture whole
    io(capture_whole(-10))
    io(capture_whole(10))
    
    -- Sequence
    io(match_sequence())
    
    io("Pattern matching stress test complete")
;

run_pattern_stress()
