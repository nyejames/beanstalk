get_map || -> {String = Int}:
    return {"Ada" = 10}
;

scores = get_map()
value = scores.get("Ada") catch:
    then 0
;
[:hashmap reachable operation]
