-- Test file: Struct field borrowing (should pass)
-- Tests that different struct fields can be borrowed independently
-- Note: This is a conceptual test - actual struct syntax may need adjustment

-- For now, simulate struct field behavior with separate variables
-- representing different fields of a conceptual struct
person_name = "Alice"
person_age ~= 30
person_email = "alice@example.com"

-- Borrow different "fields" independently - should be allowed
name_ref = person_name
age_ref = person_age
email_ref = person_email

-- Use all field references simultaneously - should work
display_name = name_ref
current_age = age_ref
contact_email = email_ref

-- After using the references, mutable access should work
person_age = person_age + 1

-- Test sequential borrowing patterns on different "fields"
name_ref2 = person_name      -- Another shared borrow of name
final_name = name_ref2       -- Use it immediately

-- After name reference is used, mutable access to age should work
person_age = person_age + 5