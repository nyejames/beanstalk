#SITE_TITLE = "Beanstalk"

badFunc |value Int|:
    BadLocal = value
    io(BadLocal)
;

person_record = |
    name String,
|

status_value :: Ok, not_ok;

badFunc(3)
