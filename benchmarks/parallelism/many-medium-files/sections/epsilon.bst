EpsilonCard = | title String, body String |

render_epsilon || -> String:
    card = EpsilonCard("Epsilon", "The benchmark keeps real Beanstalk declarations in every file.")
    return [:<article><h2>[card.title]</h2><p>[card.body]</p></article>]
;
