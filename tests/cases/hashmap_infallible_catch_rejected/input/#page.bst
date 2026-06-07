scores ~{String = Int} = {"Ada" = 10}
has_ada = scores.contains("Ada") catch:
    then false
;
[:test]
