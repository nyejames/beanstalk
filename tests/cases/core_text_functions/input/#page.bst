import @core/text {length, is_empty, contains, starts_with, ends_with}

text = "beanstalk"
len = length("abc")
empty = is_empty("")
has_stalk = contains(text, "stalk")
has_prefix = starts_with(text, "bean")
has_suffix = ends_with(text, "stalk")

io([:core_text len=[len] empty=[empty] has=[has_stalk] prefix=[has_prefix] suffix=[has_suffix]])
