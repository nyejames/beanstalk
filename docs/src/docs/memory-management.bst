#import(@libs/html/Basic)
#import(@styles/Docs)
#import(@./components)
[Basic.page:
[Docs.Navbar]

[Docs.header Basic.center: [Docs.title(1): MEMORY MANAGEMENT]]
[Basic.page:

[Basic.red: Memory management scheme still in testing / design phase ]

# Current ideas about memory management
The intention is for Beanstalk to *not* have a garbage collector.

There will be need to be safety and avoidance of common bugs without a significant penalty to how optimized the output can be.

The current plan is to have a system similar to Rust, 
but there will be more of a tradeoff of performance in exchange for simplicity.

There will be no distinct lifetime annotation syntax. 
You can return references from functions by using the parameter name in the return signature instead of just a type.
This will mean "return a reference with the same lifetime and type as this parameter".
All other lifetimes will be elided by the compiler.

Instead of explicitly annotating when values are moved, 
Beanstalk defaults to reference semantics and moves are statically determined by the compiler.

This means you only need to worry about whether you are providing a mutable reference, immutable reference or a copy of a value.

*Main Condition* 
During the reverse pass, Owners are checked for their last usage and from that point the state is set to "Moved" instead of "Owned". 
For borrows this is set back to "Owned" for the previous owner on the last use of the borrow. 

## The current idea
During the AST creation, each expression (value) is given a state of: 
- Owned
- Referenced 
- Borrowed

The compiler then does a reverse pass through the AST and checks each scope recursively. Refining the states to:
- Owned
- Referenced 
- Borrowed
- Killed
- Moved

]

