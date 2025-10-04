-- Test basic template functionality
main || -> String:
    -- Simple template with string content
    template_result ~= [: Hello World]
    return template_result
;