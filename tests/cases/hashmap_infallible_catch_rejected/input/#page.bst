scores ~{String = Int} = {"Priya" = 10}
has_ada = scores.contains("Priya") catch:
    then false
;
[:test]
