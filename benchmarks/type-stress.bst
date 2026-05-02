-- type-stress.bst
-- Type system stress test:
-- - Many struct definitions with varying field counts
-- - Complex choice types with multiple payload variants
-- - Type alias chains
-- - Receiver methods (immutable and mutable)
-- - Method call chains
-- - Struct constructor patterns (positional and named arguments)
-- - Cross-type dependencies

-- Type alias chains
# Id as Int
# UserId as Id
# AdminId as UserId
# GuestId as UserId
# SessionId as String
# TokenHash as String
# PublicKey as String

-- Collection type aliases
# Names as {String}
# Scores as {Int}
# Values as {Float}
# Records as {Record}

-- Option/Result type aliases
# MaybeInt as Int?
# MaybeString as String?
# MaybeUser as User?
# IntResult as Int, Error!
# StringResult as String, Error!

-- Simple structs
Simple = |
    value Int,
|

Pair = |
    first Int,
    second Int,
|

Triple = |
    a Int,
    b Float,
    c String,
|

-- Medium complexity structs
Person = |
    name String,
    age Int,
    email String,
    active Bool,
|

Address = |
    street String,
    city String,
    country String,
    postal_code String,
|

Coordinates = |
    latitude Float,
    longitude Float,
    altitude Float,
|

-- Large structs
UserProfile = |
    id UserId,
    username String,
    display_name String,
    email String,
    bio String,
    avatar_url String,
    verified Bool,
    follower_count Int,
    following_count Int,
    post_count Int,
|

Configuration = |
    debug_mode Bool,
    max_retries Int,
    timeout_ms Int,
    base_url String,
    api_key String,
    cache_enabled Bool,
    cache_ttl_seconds Int,
    log_level String,
|

MetricSnapshot = |
    timestamp Int,
    cpu_percent Float,
    memory_mb Float,
    disk_read_bytes Int,
    disk_write_bytes Int,
    network_rx_bytes Int,
    network_tx_bytes Int,
    process_count Int,
    thread_count Int,
    uptime_seconds Int,
|

-- Structs with type alias fields
Document = |
    id SessionId,
    title String,
    content String,
    author UserId,
    created_at Int,
    updated_at Int,
|

Session = |
    id SessionId,
    user_id UserId,
    token TokenHash,
    expires_at Int,
    created_at Int,
|

-- Cross-dependent structs
Node = |
    id Int,
    value String,
|

Edge = |
    from_id Int,
    to_id Int,
    weight Float,
|

Graph = |
    nodes {Node},
    edges {Edge},
|

Record = |
    key String,
    value String,
    timestamp Int,
    metadata String,
|

-- Choice types - simple unit variants
Status :: Ready, Loading, Complete, Failed, Cancelled;
Priority :: Low, Medium, High, Critical;
Direction :: North, South, East, West;

-- Choice types - single payload variants
OptionalInt ::
    None,
    Some | value Int |,
;

OptionalString ::
    Empty,
    Present | text String |,
;

-- Choice types - multiple payload variants
Result ::
    Success,
    Error | message String |,
    Timeout | duration_ms Int |,
    InvalidInput | field String, reason String |,
;

Event ::
    Click | x Int, y Int |,
    KeyPress | key String, modifiers String |,
    Scroll | delta_x Float, delta_y Float |,
    Resize | width Int, height Int |,
;

Message ::
    Text | content String |,
    Image | url String, alt String, width Int, height Int |,
    Video | url String, duration_ms Int |,
    File | name String, size_bytes Int, mime_type String |,
    Location | latitude Float, longitude Float, label String |,
;

-- Complex choice with many variants
Expression ::
    Literal | value String |,
    Variable | name String |,
    BinaryOp | op String, left Expression, right Expression |,
    UnaryOp | op String, operand Expression |,
    Call | name String, args {Expression} |,
    Index | target Expression, index Expression |,
    Member | target Expression, name String |,
    Conditional | condition Expression, then_expr Expression, else_expr Expression |,
    Lambda | params {String}, body Expression |,
;

-- Pattern choice
Pattern ::
    Wildcard,
    Exact | value String |,
    Range | min Int, max Int |,
    Regex | pattern String |,
    Compound | patterns {Pattern} |,
;

-- Receiver methods - immutable
get_area |this Rectangle| -> Float:
    return this.width * this.height
;

get_perimeter |this Rectangle| -> Float:
    return 2.0 * (this.width + this.height)
;

is_square |this Rectangle| -> Bool:
    return this.width is this.height
;

get_center |this Coordinates| -> Coordinates:
    return Coordinates(this.latitude, this.longitude, 0.0)
;

distance_to |this Coordinates, other Coordinates| -> Float:
    lat_diff = other.latitude - this.latitude
    lon_diff = other.longitude - this.longitude
    return ((lat_diff * lat_diff) + (lon_diff * lon_diff)).sqrt()
;

get_full_name |this Person| -> String:
    return [: [this.name] ([this.email])]
;

is_adult |this Person| -> Bool:
    return this.age >= 18
;

get_age_bucket |this Person| -> String:
    if this.age < 13:
        return "child"
    ;
    if this.age < 20:
        return "teen"
    ;
    if this.age < 65:
        return "adult"
    ;
    return "senior"
;

-- Receiver methods - mutable
bump |this ~Counter|:
    this.value = this.value + 1
;

add |this ~Counter, amount Int|:
    this.value = this.value + amount
;

reset |this ~Counter|:
    this.value = 0
;

scale |this ~Rectangle, factor Float|:
    this.width = this.width * factor
    this.height = this.height * factor
;

translate |this ~Coordinates, lat_offset Float, lon_offset Float|:
    this.latitude = this.latitude + lat_offset
    this.longitude = this.longitude + lon_offset
;

update_email |this ~Person, new_email String|:
    this.email = new_email
;

-- Method call chains
Container = |
    items {Int},
    count Int,
|

add_item |this ~Container, item Int|:
    ~this.items.push(item)
    this.count = this.count + 1
;

get_sum |this Container| -> Int:
    total = 0
    loop this.items |item|:
        total = total + item
    ;
    return total
;

get_average |this Container| -> Float:
    if this.count is 0:
        return 0.0
    ;
    sum = this.get_sum()
    return sum.to_float() / this.count.to_float()
;

NestedContainer = |
    containers {Container},
|

add_to_first |this ~NestedContainer, value Int|:
    first = this.containers.get(0) ! Container({}, 0)
    ~first.add_item(value)
;

-- Struct constructor patterns
Rectangle = |
    width Float,
    height Float,
|

Counter = |
    value Int,
|

make_square |size Float| -> Rectangle:
    return Rectangle(size, size)
;

make_named_person |name String| -> Person:
    return Person(name = name, age = 0, email = "", active = false)
;

make_default_config || -> Configuration:
    return Configuration(
        debug_mode = false,
        max_retries = 3,
        timeout_ms = 5000,
        base_url = "https://api.example.com",
        api_key = "",
        cache_enabled = true,
        cache_ttl_seconds = 300,
        log_level = "info",
    )
;

make_empty_metric || -> MetricSnapshot:
    return MetricSnapshot(
        timestamp = 0,
        cpu_percent = 0.0,
        memory_mb = 0.0,
        disk_read_bytes = 0,
        disk_write_bytes = 0,
        network_rx_bytes = 0,
        network_tx_bytes = 0,
        process_count = 0,
        thread_count = 0,
        uptime_seconds = 0,
    )
;

-- Cross-type operations
NodeLookup = |
    nodes {Node},
|

find_node |this NodeLookup, id Int| -> Node?:
    loop this.nodes |node|:
        if node.id is id:
            return node
        ;
    ;
    return none
;

add_node |this ~NodeLookup, node Node|:
    ~this.nodes.push(node)
;

EdgeLookup = |
    edges {Edge},
|

find_edges_from |this EdgeLookup, from_id Int| -> {Edge}:
    result ~= {}
    loop this.edges |edge|:
        if edge.from_id is from_id:
            ~result.push(edge)
        ;
    ;
    return result
;

-- Runtime type operations
process_person |person Person| -> String:
    name = person.get_full_name()
    bucket = person.get_age_bucket()
    adult = person.is_adult()
    return [: [name] - [bucket] - adult: [adult]]
;

process_rectangle |rect Rectangle| -> String:
    area = rect.get_area()
    perim = rect.get_perimeter()
    square = rect.is_square()
    return [: area: [area], perimeter: [perim], square: [square]]
;

process_session |session Session| -> String:
    return [: Session [session.id] for user [session.user_id]]
;

process_document |doc Document| -> String:
    return [: Document "[doc.title]" by [doc.author]]
;

process_metric |metric MetricSnapshot| -> String:
    return [: CPU: [metric.cpu_percent]%, Mem: [metric.memory_mb]MB]
;

-- Stress test entry
run_type_stress ||:
    io("Type system stress test starting")
    
    -- Create instances
    rect = Rectangle(10.0, 20.0)
    square = make_square(15.0)
    counter = Counter(0)
    person = Person("Alice", 30, "alice@example.com", true)
    config = make_default_config()
    metric = make_empty_metric()
    
    -- Call immutable methods
    area = rect.get_area()
    perim = rect.get_perimeter()
    is_sq = rect.is_square()
    io([: Rectangle: area=[area], perimeter=[perim], square=[is_sq]])
    
    name = person.get_full_name()
    bucket = person.get_age_bucket()
    io([: Person: [name], bucket=[bucket]])
    
    -- Call mutable methods
    ~counter.bump()
    ~counter.add(5)
    ~counter.bump()
    io([: Counter: [counter.value]])
    
    ~rect.scale(2.0)
    new_area = rect.get_area()
    io([: Scaled rectangle: [new_area]])
    
    -- Collection of structs
    container = Container({}, 0)
    ~container.add_item(10)
    ~container.add_item(20)
    ~container.add_item(30)
    sum = container.get_sum()
    avg = container.get_average()
    io([: Container: sum=[sum], avg=[avg]])
    
    -- Choice construction
    ready = Status::Ready
    loading = Status::Loading
    
    opt_int = OptionalInt::Some(42)
    opt_str = OptionalString::Present("hello")
    
    result_success = Result::Success
    result_error = Result::Error("something went wrong")
    
    event_click = Event::Click(100, 200)
    event_key = Event::KeyPress("Enter", "ctrl")
    
    io("Type system stress test complete")
;

-- Run the stress test
run_type_stress()
