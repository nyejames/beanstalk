#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:
[Navbar]

[header center: [title(1): MEMORY MANAGEMENT]]
[main:

[red: Memory management scheme still in testing / design phase ]

# Current ideas about memory management
The intention is for Beanstalk to *not* have a garbage collector.

There will be need to be safety and avoidance of common bugs without a significant penalty to how optimized the output can be.

The current plan is to have a system similar to Rust, 
with some additional syntax simplicity and a "polonius" style borrow checker.

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

*Main Condition* 
During the reverse pass, Owners are checked for their last usage and from that point the state is set to "Moved" instead of "Owned". 
For borrows this is set back to "Owned" for the previous owner on the last use of the borrow. 

*Next condition* 
When setting something to "moved" or back to "owned",
if this is the first time the move or return is encountered, 
instead this will be set to "Killed". 

*Immutable Borrows*
In a similar way, the last use of an immutable borrow must be set to "Killed". 
This will allow mutable borrows or moves to happen after the last use of an immutable borrow.
If the immutable borrow is not "killed" and a value is still "referenced" or "borrowed",
trying to mutably borrow or move will result in an error.

*What this also implies for free*
If the value was passed into the current scope as a borrow to a function call or new scope, 
this means that the value definitely needs to be returned to the owning scope as this analysis for that scope as it is known to be used later. 
Borrows are only set to "killed" if they were owned by something inside the current scope and this was their last usage.

*Lifetimes*
- Functions can define in their signatures that they are giving back ownership of a value or giving back the same referece. 
This is signalled via using the same name as a parameter in the return type (instead of using a type).
- Functions never drop borrowed or moved values,
their calling scope will drop them after the function returns if the values were moved into the function.

*Overview*
Analysis takes places across the AST in reverse. 
Each branch will need to independently check for liveness based on it's own ownership rules.
Every scope boundary will be analyzed as a move or borrow into that scope and then that scope is recursively checked.


]

