max_value #Int = cast "9007199254740991" catch:
    then 0
;

min_value #Int = cast "-9007199254740991" catch:
    then 0
;

[: const safe int boundaries: [max_value]/[min_value]]
