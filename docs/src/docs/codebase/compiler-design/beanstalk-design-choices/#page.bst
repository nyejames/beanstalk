import @./beanstalk-design-choices {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "02. Why Beanstalk makes different choices"
page_description #= "The design trade-offs behind Beanstalk's language and compiler."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 02. Why Beanstalk makes different choices

    # Why Beanstalk makes different choices

    Beanstalk keeps the source language small and moves complexity into the compiler on purpose. This article names the bias before the pipeline walk begins.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../what-a-compiler-does (Previous) / @../starting-a-build (Next)
]
