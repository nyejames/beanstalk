import @./traits { RENDERABLE }

Card = |
    title String,
|

render |this Card| -> String:
    return this.title
;

Card must RENDERABLE

io("imported trait reference resolved")
