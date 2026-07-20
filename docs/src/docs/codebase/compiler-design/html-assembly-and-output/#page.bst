import @./html-assembly-and-output {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "18. HTML assembly and output"
page_description #= "How the HTML builder assembles and owns the generated page and assets."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 18. HTML assembly and output

    # Building and writing the final page

    The HTML builder assembles records into a document and owned output set, ending at the greeting fragment.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../backend-lowering (Previous)
]
