-- This is a library of basic HTML components and styles
-- This contains a css reset and classes for some core beanstalk formatters
Document = |
    prelude String = "<!DOCTYPE html>",
    en String = [$html:<html lang="en">],
    head String = [$html:
      <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta name="color-scheme" content="light dark" />
        [$slot]
      </head>
    ],
    title String = [$html:<title style="[$slot("style")]">[$slot]</title>],
    style String = [$html:<style>[default_style][$slot]</style>],
    body String = [$html:<body style="[$slot("style")]">[$slot]</body>],
    div String = [$html:<div style="[$slot("style")]">[$slot]</div>],
|

-- Export a constant instance of Document
-- Using all of its default values
# doc = Document()

Format = |
    -- Basic formatting
    underline String = [$html:<u style="[$slot("style")]">[$slot]</u>],
    strikethrough String = [$html:<s style="[$slot("style")]">[$slot]</s>],
    table String = [$html:
      <table style="[$slot("style")]">
        [$slot]
      </table>
    ],
|

# format = Format()

-- Inline Utility Styles
# center String = [$insert("style"):text-align: center;]
# right String = [$insert("style"):text-align: right;]
# left String = [$insert("style"):text-align: left;]

-- Basic HTML components
# img String = [$html:<img src="[$slot("src")]" style="[$slot("style")]" alt="[$slot]">]

-- A css reset and some classes for the built-in code formatter
-- This is inspired by Pico.css, but is an even more minimal reset.
-- Actual style systems are intended to be built on top of this easily.
# default_style = [$css:
    :root {
      color-scheme: light dark;

      --bst-font-size: 100%;

      /* Default Background Colours */
      --bst-bg-lightmode: #fff;
      --bst-bg-darkmode: #13171f;
      --bst-text-lightmode: #373c44;
      --bst-text-darkmode: #e0e3e7;

      --bst-font-family-emoji: "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";
      --bst-font-family-sans-serif: system-ui, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, Helvetica, Arial, "Helvetica Neue", sans-serif, var(--bst-font-family-emoji);
      --bst-font-family-monospace: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace, var(--bst-font-family-emoji);
      --bst-font-family: var(--bst-font-family-sans-serif);

      --bst-line-height: 1.5;
      --bst-font-weight: 400;
      --bst-font-size: 100%;
      --bst-text-underline-offset: 0.1rem;
      --bst-border-radius: 0.25rem;
      --bst-border-width: 0.0625rem;
      --bst-outline-width: 0.125rem;
      --bst-transition: 0.2s ease-in-out;

      --bst-spacing: 1rem;
      --bst-spacing-small: 0.5rem;

      /* Code block colours */
      --comment-dark: #838c86;
      --comment-light: #759061;
      --keyword-dark: #ea943e;
      --keyword-light: #ad590e;
      --string-dark: #4fba11;
      --string-light: #007130;
      --number-dark: #96b5ed;
      --number-light: #073f85;
      --operator-dark: #9ec1ff;
      --operator-light: #2b476a;
      --struct-dark: #a17fbd;
      --struct-light: #ac2187;
      --type-dark: #54c5d3;
      --type-light: #0d8db3;
      --parenthesis-dark: #bcac52;
      --parenthesis-light: #ad7e0d;
    }

    /* where: is used here for less specificity, allowing easier overriding */
    :where(:root) {
      -webkit-tap-highlight-color: transparent;
      -webkit-text-size-adjust: 100%;
      -moz-text-size-adjust: 100%;
      text-size-adjust: 100%;
      font-weight: var(--bst-font-weight);
      font-size: var(--bst-font-size);
      line-height: var(--bst-line-height);
      font-family: var(--bst-font-family);
      text-underline-offset: var(--bst-text-underline-offset);
      text-rendering: optimizeLegibility;
      overflow-wrap: break-word;
      -moz-tab-size: 4;
      -o-tab-size: 4;
      tab-size: 4;

    }

    /* Container mostly provides default padding for content sections */
    .container {
      width: 100%;
      margin-right: auto;
      margin-left: auto;
      padding-right: var(--bst-spacing-small);
      padding-left: var(--bst-spacing-small);
    }

    @media (min-width: 576px) {
      :root {
        --bst-font-size: 106.25%;
      }
      .container {
        max-width: 510px;
        padding-right: var(--bst-spacing);
        padding-left: var(--bst-spacing);
        padding-bottom: var(--bst-spacing);
      }
    }
    @media (min-width: 768px) {
      :root {
        --bst-font-size: 112.5%;
      }
      .container {
        max-width: 700px;
      }
    }
    @media (min-width: 1024px) {
      :root {
        --bst-font-size: 118.75%;
      }
      .container {
        max-width: 950px;
      }
    }
    @media (min-width: 1280px) {
      :root {
        --bst-font-size: 125%;
      }
      .container {
        max-width: 1200px;
      }
    }
    @media (min-width: 1536px) {
      :root {
        --bst-font-size: 131.25%;
      }
      .container {
        max-width: 1450px;
      }
    }

    body {
      width: 100%;
      margin: 0;
    }

    /* Common Styles */
    .bst-center {
      text-align: center;
    }

    /* Nav Style */
    .bst-nav-0 {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 1rem;
    }
    .bst-nav-1 {
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-bottom-color: grey;
      border-bottom-style: solid;
      border-bottom-width: 1px;
      padding: 0rem 2rem 0rem 2rem;
    }

    /* Title Style */
    .bst-title-0 {
      font-size: 3.5rem;
      font-weight: bold;
      letter-spacing: 0.4rem;
      word-spacing: 0.5rem;
      margin: 1.5rem 0 1.4rem 0;
      line-height: 3.7rem;
    }
    .bst-title-1 {
      font-size: 3rem;
      font-weight: bold;
      letter-spacing: 0.3rem;
      word-spacing: 0.4rem;
      margin: 1.3rem 0 1.2rem 0;
      line-height: 3.5rem;
    }
    .bst-title-2 {
      font-size: 2.5rem;
      font-weight: bold;
      letter-spacing: 0.3rem;
      word-spacing: 0.35rem;
      margin: 1.2rem 0 1rem 0;
      line-height: 3.3rem;
    }

    /* Code Style */
    .codeblock {
      color: #222222;
      color: light-dark(#222222, #ddd);
      font-family: var(--bst-font-family-monospace);
      white-space: break-spaces;
      tab-size: 4;
    }
    .bst-code-comment {
      color: var(--comment-light);
      color:light-dark(var(--comment-light), var(--comment-dark))
    }
    .bst-code-keyword {
      color: var(--keyword-light);
      color:light-dark( var(--keyword-light), var(--keyword-dark))
    }
    .bst-code-string {
      color: var(--string-light);
      color:light-dark( var(--string-light), var(--string-dark))
    }
    .bst-code-number {
      color: var(--number-light);
      color:light-dark( var(--number-light), var(--number-dark))
    }
    .bst-code-operator {
      color: var(--operator-light);
      color:light-dark( var(--operator-light), var(--operator-dark))
    }
    .bst-code-struct {
      color: var(--struct-light);
      color:light-dark( var(--struct-light), var(--struct-dark))
    }
    .bst-code-type {
      color: var(--type-light);
      color:light-dark( var(--type-light), var(--type-dark))
    }
    .bst-code-parenthesis {
      color: var(--parenthesis-light);
      color:light-dark( var(--parenthesis-light), var(--parenthesis-dark))
    }

    /* BASIC CSS RESET STUFF */

    /**
     * Document
     * Content-box & Responsive typography
    */
    *,
    *::before,
    *::after {
      box-sizing: border-box;
      background-repeat: no-repeat;
    }

    ::before,
    ::after {
      text-decoration: inherit;
      vertical-align: inherit;
    }
]
