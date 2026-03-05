#single_slot = [: before [..] after]
#wrapped_content = [single_slot: this content is now wrapped]

#basic_slots = [: before [..] in the middle [..] afterwards]
#template_using_slots = [basic_slots:
    this text goes before any slots
    [$1: This goes into the first slot]
    this would go in between the slots
    [$2: This goes into the second slot]
    this would go after any slots
]

io(wrapped_content)
io(template_using_slots)
