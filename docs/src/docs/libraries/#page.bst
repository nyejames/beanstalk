import @./import-paths {
    content as import_paths,
}

import @./import-paths-basic {
    content as import_paths_basic,
}

import @./grouped-and-namespace-imports {
    content as grouped_and_namespace_imports,
}

import @./grouped-and-namespace-imports-basic {
    content as grouped_and_namespace_imports_basic,
}

import @./module-visibility {
    content as module_visibility,
}

import @./module-visibility-basic {
    content as module_visibility_basic,
}

import @./public-reexports {
    content as public_reexports,
}

import @./public-reexports-basic {
    content as public_reexports_basic,
}

import @./source-libraries {
    content as source_libraries,
}

import @./source-libraries-basic {
    content as source_libraries_basic,
}

import @./core-builder-and-external-packages {
    content as core_builder_and_external_packages,
}

import @./core-builder-and-external-packages-basic {
    content as core_builder_and_external_packages_basic,
}

import @styles/docs {
    navbar,
    section,
    language_theme_head,
    doc_level,
    doc_pager,
}

page_title #= "Libraries and imports"
page_description #= "Import paths, module visibility, source libraries, and builder-provided packages."
page_head #= language_theme_head

#[navbar]

#[section, $md:
    @../ (Documentation) / Libraries and imports

    # Libraries and imports

    Imports make names visible. Module roots decide what crosses a module boundary.
    Source libraries give reusable modules stable prefixes.

    Those are separate jobs. Keeping them separate is what stops a filesystem from
    quietly becoming an API.
]

#[section:
    [doc_level:
        [$insert("key"):import-paths]
        [$insert("title"):Import paths]
        [$insert("advanced"):[import_paths]]

        [import_paths_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):grouped-and-namespace-imports]
        [$insert("title"):Grouped and namespace imports]
        [$insert("advanced"):[grouped_and_namespace_imports]]

        [grouped_and_namespace_imports_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):module-visibility]
        [$insert("title"):Module visibility]
        [$insert("advanced"):[module_visibility]]

        [module_visibility_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):public-reexports]
        [$insert("title"):Public re-exports]
        [$insert("advanced"):[public_reexports]]

        [public_reexports_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):source-libraries]
        [$insert("title"):Source libraries]
        [$insert("advanced"):[source_libraries]]

        [source_libraries_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):core-builder-and-external-packages]
        [$insert("title"):Core, builder and external packages]
        [$insert("advanced"):[core_builder_and_external_packages]]

        [core_builder_and_external_packages_basic]
    ]
]

#[section:
    [doc_pager:
        [$insert("previous_path"):../project-structure/]
        [$insert("previous_title"):Project structure]
        [$insert("next_path"):../beandown/]
        [$insert("next_title"):Beandown]
    ]
]