We are building a compiler. The language is called Beanstalk. It has a simple type system and minimal syntax that doesn't try to have zero cost abstractions for everything. It will use Cranelift as the backend for Codegen. Some of the features below are not yet implemented in the compiler. Here is a basic syntax overview:
# Quick Synax Overview
The specifics are not fully decided on, but some of the key things are:
- Colon opens a scope, semicolon closes it.
- No use of square brackets for arrays, curly braces are used instead. Square brackets are only used for string templates.
- Equality and other logical operators only use keywords (you can't use == or ! for logical expressions for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit). This comes before the type.
- Double dashes for single line comments (--)
- 4 spaces are recommended for indentation. 
- Types / Objects use Upper_Snake_Case.
- Everything else uses regular_snake_case
# Declarations
int ~= 0
float ~= 0.0
string ~= "boo!"
char ~= '😊'
raw_string ~= `hi`
template ~= []
bool ~= true
mutable_collection ~= {}
immutable_collection = {}
struct_definition = |
    value Int,
    another_value Float,
|
# Comments
Comments use a double minus sign '--'.
# Functions
get_doubled |value Int| -> Int:
    return value *= 2
;
-- Using the name of a variable passed in
-- means it's returning the original reference.
multipleReturns |x Int| -> x, Bool:
    value = x + 1
    return (value, value is > 0)
;
func_call_that_can_return_an_error() !:
    -- Error handling code
;
-- This function can return a String or None
parent_func () -> String, Error!:
    return func_call_that_can_return_an_error() !err:
        print("Error: ", err)
        return "", err
    ;
;
## Using the ? operator
getURL || -> String:
    return getNetworkRequest() ? ""
;
-- Returns a string or None
getURL || -> String?:
    return getNetworkRequest()
;
## Control fLow
v = 1
if v is not 1:
    print("nope")
else
    print("yep")
;
for -20 to big_number:
    print("hello")
;
for item, index in collection:
    print(index.to_string())
;
-- Pattern matching is exhaustive
thing = 69
if thing is:
    0: print("zero")
    < 0: print("negative")
    > 1: print("big number")
    not 5: print("not 5")
    else: print("only one")
;
# Templates
Beanstalk has unique, superpowered templating strings. Templates have two sections, the head and the body, separated with a colon. Templates are scoped using square brackets and can be passed special types called Styles to change their behavior, content and the way the string is parsed (e.g ran through a markdown parser). The main purpose of a template is to generate either text content, markdown, HTML or code.
## The head section
Is used to:
- Define properties of the template
- Convert literals and expressions to strings to be added into the template
- Wrap the template itself in strings that conditionally depend on the template's properties or parent template's properties.
#### The body section
is used to define the content of the template. This is a string. Inside the body of a template, you can nest more templates using square brackets. 
## Beanstalk Markdown
Beanstalk uses it's own flavour of markdown that can be used inside template bodies if the template has the markdown option enabled. Instead of writing any raw HTML, you can use the Beanstalk template syntax to write your text content inside of a template body. Beanstalk markdown supports the familiar heading, bold and italic syntax from most markdown parsers.
## Beanstalk Template example
This is using the built in #markdown library that will become part of the compiler. This imports functions such as rgb() into the template so any child scenes can see and use those functions. Templates capture any variables in their surrounding scope, so can be passed into scene heads.

[Section #markdown:
    [head_of_element: content of element that renders as text (markdown / metadata / alt tag / code ..etc)]

    [img("images/meme.jpg"): meme (this text in the template body is the alt description automatically)]

    # A Cool Section
    Using the export symbol @ you can give the template an ID in the form of a variable name. 
    The @ is needed to tell the compiler this is a declaration of the ID. 
    ID's can be created for templates at runtime but cannot be changed once a template is created.

    [section @template_id:
        [ variable ]
        [link("example.com/page"): here's a link!]
    ]

    [ for text in text_array: 
        [ text ]
    ]

    [table(2):
        [: Heading 1] [: Heading 2]
        [: text] [: text]
    ] 

    [if not text.isBlank():
        [ text: plus some extra text in the same element as the variable text ]
    ]

    [rgb(230,20,150): this is a bunch of text with rgb color]

    [ imported_template ]

]
# Memory Management
Beanstalk uses a borrow checker but will not have any explicit lifetime annotations. There is no GC or manual memory management. It will assume the shortest lifetime when it can't be inferred. References can be returned from functions if they are a named return. Only named parameters that were passed in can be passed back out as references. Functions are pass by reference, and using the ~ symbol will mean it's passed by mnutable reference (the compiler will determine whether to move instead). There is no distinction in the syntax between a mutable reference and a move. The compiler will statically analyse when it can move a variable rather than just give over a mutable reference by checking for it's last use. Beanstalk may implement ARC for edge cases or situations where lifetimes can't be figured out at all, or to eliviate some strictness in certain places.