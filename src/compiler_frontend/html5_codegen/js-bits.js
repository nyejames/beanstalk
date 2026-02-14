// For loading WASM
WebAssembly.instantiateStreaming(fetch("./wasm-module-name.wasm")).then((obj)=>{
let wsx = obj.instance.exports;//js
;});

// WASM will need a JS function to update the DOM
// Functions that shouldn't be passed to WASM directly, so may need glue code
function uInnerHTML(id,update){
    const es = document.getElementsByClassName(id);
    if(Array.isArray(update)){update=update.join(' ')}
    for(let i = 0;i<es.length;i++){
        es[i].innerHTML=update
    }
}
function uAppendChild(id,update){
    const es = document.getElementsByClassName(id);
    if(Array.isArray(update)){update=update.join(' ')}
    for(let i = 0;i<es.length;i++){
        es[i].appendChild(update)
    }
}
function uRemoveChild(id,update){
    const es = document.getElementsByClassName(id);
    if(Array.isArray(update)){update=update.join(' ')}
    for(let i = 0;i<es.length;i++){
        es[i].removeChild(update)
    }
}
function uReplaceChild(id,update){
    const es = document.getElementsByClassName(id);
    if(Array.isArray(update)){update=update.join(' ')}
    for(let i = 0;i<es.length;i++){
        es[i].replaceChild(update)
    }
}

