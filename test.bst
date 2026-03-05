-- RUNTIME

-- Single slot example
single_slot = [: before [..] after]

wrapped_content = [single_slot: this content is now wrapped]

io(wrapped_content)

-- Multiple slots example
basic_slots = [: before [..] in the middle [..] afterwards]

template_using_slots = [basic_slots:
    this text goes before any slots
    [$1: This goes into the first slot]
    this would go in between the slots
    [$2: This goes into the second slot]
    this would go after any slots
]

io(template_using_slots)

-- COMPILETIME

-- Single slot example
# single_slot2 = [: BEFORE [..] AFTER]

# wrapped_content2 = [single_slot2: this content is now wrapped]

-- Create runtime top level template
[wrapped_content2:<br><br><br>]

-- Multiple slots example
# basic_slots2 = [: BEFORE [..] MIDDLE [..] AFTER]

# template_using_slots2 = [basic_slots2:
    this text goes before any slots
    [$1: This goes into the first slot]
    this would go in between the slots
    [$2: This goes into the second slot]
    this would go after any slots
]

#[template_using_slots2]