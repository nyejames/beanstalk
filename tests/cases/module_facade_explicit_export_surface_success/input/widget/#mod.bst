import @./theme { accent }

[: imported-root-runtime-must-not-execute ]

private_prefix #= "facade"

PrivateLabel = |
    text String,
|

make_private_label |text String| -> PrivateLabel:
    return PrivateLabel([: [private_prefix]-[text]-[accent]])
;

export PublicCard = |
    title String,
|

export PublicState ::
    Ready,
;

export PublicText as String

export public_title #= "card"

export make_state || -> PublicState:
    return PublicState::Ready
;

export render_card |card PublicCard| -> PublicText:
    label = make_private_label(card.title)
    return label.text
;
