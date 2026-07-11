wrapper #= [$children([:<li>W[$slot]Z</li>]):
<ul>
[$slot]
</ul>
]

md #= [$md:
# Docs
]

code_block #= [$code("bst"):
loop x
]

raw_block #= [$raw:
  keep
]

#[wrapper:
[md]
[code_block]
[raw_block]
[$fresh: [:<fresh-item/>]]
]
