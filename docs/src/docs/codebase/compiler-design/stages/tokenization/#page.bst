import @./tokenization {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 1: Tokenization"
page_description #= "How Beanstalk turns source text into precise, located tokens."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 1: Tokenization

    # Stage 1: Tokenization

    The tokenizer turns source text into located tokens. It stays focused on spelling and boundaries so later stages can deal with meaning.
]

#[section, reference_content]
