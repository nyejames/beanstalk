-- Prove Float() constructor-style conversion is rejected with removed-constructor diagnostic.

value = Float("not_a_float") catch:
    then 9.9
;
