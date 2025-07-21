#![allow(dead_code)]
pub enum DOMUpdate {
    InnerHTML,
    AppendChild,
    RemoveChild,
    ReplaceChild,
}

pub fn generate_dom_update_js(update: DOMUpdate) -> &'static str {
    /*
        JS Functions that accept the classname of an element
        and what the new updated value will be.

        There is a different function for each type of DOM update,
        so only the necessary functions should be added at compile time into the JS output.

        The compiler will only generate the necessary JS functions for each kind type of DOM update that is needed in the program.

        Uses classname so it can update all elements referencing the same variable
    */

    match update {
        DOMUpdate::InnerHTML => {
            r#"
function uInnerHTML(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ')
    }
    
    for (let i = 0; i < elements.length; i++) {
        elements[i].innerHTML = update
    }
}"#
        }
        DOMUpdate::AppendChild => {
            r#"
function uAppendChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ')
    }
    
    for (let i = 0; i < elements.length; i++) {
        elements[i].appendChild(update)
    }
}"#
        }
        DOMUpdate::RemoveChild => {
            r#"
function uRemoveChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ')
    }
    
    for (let i = 0; i < elements.length; i++) {
        elements[i].removeChild(update)
    }
}"#
        }
        DOMUpdate::ReplaceChild => {
            r#"
function uReplaceChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ')
    }
    
    for (let i = 0; i < elements.length; i++) {
        elements[i].replaceChild(update)
    }
}"#
        }
    }
}
