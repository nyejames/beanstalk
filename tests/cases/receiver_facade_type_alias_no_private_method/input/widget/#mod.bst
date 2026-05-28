import @./impl { Counter, make_counter_impl }

PublicCounter as Counter

make_counter || -> PublicCounter:
    return make_counter_impl()
;
