-- collection-stress.bst
-- Collection operations stress test:
-- - Collection mutations (push, set, remove)
-- - Nested loops over collections
-- - Collection iteration with index binding
-- - Result fallback patterns (get(index) ! default)
-- - Range loops (ascending, descending, with by step)
-- - Mixed collection and range loop nesting

-- Simple struct for collection items
Item = |
    id Int,
    value String,
|

Record = |
    key String,
    count Int,
    score Float,
|

-- Basic collection operations
basic_mutations ||:
    io("Basic collection mutations")
    
    -- Create empty collection
    numbers ~{Int} = {}
    
    -- Push operations
    loop 0 to 20 |i|:
        ~numbers.push(i * 3)
    ;
    
    io([: Pushed [numbers.length()] items])
    
    -- Set operations
    ~numbers.set(0, 999)
    ~numbers.set(5, 777)
    ~numbers.set(10, 555)
    
    -- Remove operations
    ~numbers.remove(3)
    ~numbers.remove(7)
    
    io([: After mutations: [numbers.length()] items])
;

-- Get with fallback patterns
get_with_fallback ||:
    io("Get with fallback patterns")
    
    items ~= {10, 20, 30, 40, 50}
    
    -- Valid gets
    first = items.get(0) ! 0
    middle = items.get(2) ! 0
    last = items.get(4) ! 0
    
    io([: Valid: first=[first], middle=[middle], last=[last]])
    
    -- Invalid gets with fallbacks
    out_of_bounds = items.get(100) ! -1
    negative = items.get(-1) ! -1
    
    io([: Invalid: out_of_bounds=[out_of_bounds], negative=[negative]])
;

-- Nested loops over collections
nested_collection_loops ||:
    io("Nested collection loops")
    
    outer ~{Int} = {}
    loop 0 to 5 |i|:
        ~outer.push(i)
    ;
    
    inner ~{Int} = {}
    loop 0 to 4 |j|:
        ~inner.push(j * 10)
    ;
    
    result ~{String} = {}
    
    loop outer |o, oi|:
        loop inner |i, ii|:
            ~result.push([: o=[o]([oi]) i=[i]([ii])])
        ;
    ;
    
    io([: Generated [result.length()] combinations])
;

-- Index binding in loops
index_binding ||:
    io("Index binding in loops")
    
    items ~= {"alpha", "beta", "gamma", "delta", "epsilon"}
    
    output ~= ""
    loop items |item, index|:
        if index is not 0:
            output = [: [output], ]
        ;
        output = [: [output][index]:[item]]
    ;
    
    io([: Items: [output]])
;

-- Range loops - ascending
ascending_ranges ||:
    io("Ascending range loops")
    
    result_1 ~= ""
    loop 0 to 5 |i|:
        result_1 = [: [result_1][i] ]
    ;
    io([: 0 to 5: [result_1]])
    
    result_2 ~= ""
    loop 0 to & 5 |i|:
        result_2 = [: [result_2][i] ]
    ;
    io([: 0 to & 5: [result_2]])
    
    result_3 ~= ""
    loop 10 to 20 by 2 |i|:
        result_3 = [: [result_3][i] ]
    ;
    io([: 10 to 20 by 2: [result_3]])
;

-- Range loops - descending
descending_ranges ||:
    io("Descending range loops")
    
    result_1 ~= ""
    loop 10 to 0 |i|:
        result_1 = [: [result_1][i] ]
    ;
    io([: 10 to 0: [result_1]])
    
    result_2 ~= ""
    loop 10 to & 0 by 2 |i|:
        result_2 = [: [result_2][i] ]
    ;
    io([: 10 to & 0 by 2: [result_2]])
    
    result_3 ~= ""
    loop 50 to 10 by 5 |i|:
        result_3 = [: [result_3][i] ]
    ;
    io([: 50 to 10 by 5: [result_3]])
;

-- Float ranges
float_ranges ||:
    io("Float range loops")
    
    result ~= ""
    loop 0.0 to 1.0 by 0.25 |f|:
        result = [: [result][f] ]
    ;
    io([: 0.0 to 1.0 by 0.25: [result]])
;

-- Mixed collection and range nesting
mixed_nesting ||:
    io("Mixed collection and range nesting")
    
    matrix ~{{Int}} = {}
    
    -- Build matrix with range loops
    loop 0 to 5 |row|:
        row_items ~{Int} = {}
        loop 0 to 5 |col|:
            ~row_items.push(row * 10 + col)
        ;
        ~matrix.push(row_items)
    ;
    
    -- Process matrix with collection loops
    sum = 0
    loop matrix |row, ri|:
        loop row |val, ci|:
            sum = sum + val
        ;
    ;
    
    io([: Matrix sum: [sum]])
;

-- Collection of structs
struct_collection ||:
    io("Collection of structs")
    
    items ~{Item} = {}
    
    -- Add items
    loop 0 to 10 |i|:
        item = Item(i, [: item-[i]])
        ~items.push(item)
    ;
    
    -- Update items
    loop 0 to 5 |i|:
        updated = Item(i * 100, [: updated-[i]])
        ~items.set(i, updated)
    ;
    
    -- Remove items
    ~items.remove(2)
    ~items.remove(5)
    
    -- Process remaining
    total_id = 0
    loop items |item|:
        total_id = total_id + item.id
    ;
    
    io([: Total id: [total_id], Count: [items.length()]])
;

-- Deep nesting (3+ levels)
deep_nesting ||:
    io("Deep nesting (3+ levels)")
    
    outer ~{Int} = {}
    loop 0 to 3 |o|:
        ~outer.push(o)
    ;
    
    middle ~{Int} = {}
    loop 0 to 3 |m|:
        ~middle.push(m * 10)
    ;
    
    inner ~{Int} = {}
    loop 0 to 3 |i|:
        ~inner.push(i * 100)
    ;
    
    combinations = 0
    loop outer |o|:
        loop middle |m|:
            loop inner |i|:
                combinations = combinations + 1
            ;
        ;
    ;
    
    io([: Deep nesting combinations: [combinations]])
;

-- Collection as accumulator
accumulator_pattern ||:
    io("Collection as accumulator")
    
    result ~{String} = {}
    
    loop 0 to 20 |i|:
        if i % 2 is 0:
            ~result.push([: even-[i]])
        ;
        if i % 3 is 0:
            ~result.push([: triple-[i]])
        ;
        if i % 5 is 0:
            ~result.push([: fifth-[i]])
        ;
    ;
    
    io([: Accumulated [result.length()] items])
    
    -- Remove every third item
    index = 0
    to_remove ~{Int} = {}
    loop result |_, idx|:
        if idx % 3 is 0:
            ~to_remove.push(idx)
        ;
    ;
    
    -- Remove in reverse order to maintain indices
    loop to_remove |ri|:
        ~result.remove(ri)
    ;
    
    io([: After removal: [result.length()] items])
;

-- Conditional collection building
conditional_building ||:
    io("Conditional collection building")
    
    positives ~{Int} = {}
    negatives ~{Int} = {}
    
    loop -20 to 21 |n|:
        if n > 0:
            ~positives.push(n)
        ;
        if n < 0:
            ~negatives.push(n)
        ;
    ;
    
    io([: Positives: [positives.length()], Negatives: [negatives.length()]])
;

-- Search patterns
search_patterns ||:
    io("Search patterns")
    
    haystack ~= {15, 23, 8, 42, 16, 4, 99, 31, 7, 55}
    
    -- Find first even number > 20
    found = -1
    loop haystack |value, index|:
        if found is -1:
            if value > 20:
                if value % 2 is 0:
                    found = value
                ;
            ;
        ;
    ;
    
    io([: Found: [found]])
;

-- Collection transformation
transformation ||:
    io("Collection transformation")
    
    source ~= {1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
    doubled ~{Int} = {}
    squared ~{Int} = {}
    
    loop source |n|:
        ~doubled.push(n * 2)
        ~squared.push(n * n)
    ;
    
    sum_doubled = 0
    loop doubled |d|:
        sum_doubled = sum_doubled + d
    ;
    
    sum_squared = 0
    loop squared |s|:
        sum_squared = sum_squared + s
    ;
    
    io([: Sum doubled: [sum_doubled], Sum squared: [sum_squared]])
;

-- Record collection operations
record_collection ||:
    io("Record collection operations")
    
    records ~{Record} = {}
    
    -- Build records
    loop 0 to 10 |i|:
        r = Record([: key-[i], i * 10, i.to_float() * 2.5)
        ~records.push(r)
    ;
    
    -- Process records
    total_count = 0
    total_score = 0.0
    
    loop records |rec|:
        total_count = total_count + rec.count
        total_score = total_score + rec.score
    ;
    
    io([: Total count: [total_count], Total score: [total_score]])
;

-- Complex collection workflow
complex_workflow ||:
    io("Complex collection workflow")
    
    -- Stage 1: Build initial collection
    stage1 ~{Int} = {}
    loop 0 to 100 |i|:
        ~stage1.push(i)
    ;
    
    -- Stage 2: Filter and transform
    stage2 ~{Int} = {}
    loop stage1 |n|:
        if n % 7 is 0:
            ~stage2.push(n * 3)
        ;
    ;
    
    -- Stage 3: Further processing
    stage3 ~{String} = {}
    loop stage2 |n, idx|:
        ~stage3.push([: item-[idx]=[n]])
    ;
    
    -- Stage 4: Final aggregation
    result ~= ""
    loop stage3 |s|:
        result = [: [result][s]; ]
    ;
    
    io([: Pipeline result: [stage3.length()] items])
;

-- Stress test entry
run_collection_stress ||:
    io("Collection stress test starting")
    
    basic_mutations()
    get_with_fallback()
    nested_collection_loops()
    index_binding()
    ascending_ranges()
    descending_ranges()
    float_ranges()
    mixed_nesting()
    struct_collection()
    deep_nesting()
    accumulator_pattern()
    conditional_building()
    search_patterns()
    transformation()
    record_collection()
    complex_workflow()
    
    io("Collection stress test complete")
;

run_collection_stress()
