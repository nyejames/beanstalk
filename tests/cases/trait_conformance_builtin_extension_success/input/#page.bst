display |this Int| -> String:
    return [:int=[this]]
;

Int must DISPLAYABLE

value = 7
[:trait_conformance_builtin_extension_success [value.display()]]
