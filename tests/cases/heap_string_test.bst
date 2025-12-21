-- Test heap-allocated string handling
template_string ~= [:This is a runtime template]
io(template_string)