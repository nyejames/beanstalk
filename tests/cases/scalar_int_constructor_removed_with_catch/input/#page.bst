-- Prove Int() constructor-style conversion is rejected with removed-constructor diagnostic.

value = Int("not_a_number") catch:
    then 99
;
