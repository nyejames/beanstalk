import @components/card {Card, render_card}
import @components/listing {render_listing}
import @data/constants {app_title}

render_page |card Card| -> String:
    cards = {card, Card("Nested", "Grouped import path")}
    return [:
        <main>
            <h1>[app_title]</h1>
            [render_card(card)]
            [render_listing(cards)]
        </main>
    ]
;

