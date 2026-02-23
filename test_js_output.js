function test_js_output_bst_start() {
    let _template;
    let x;
    let y;
    let __hir_tmp_0;
    
    _template = "";
    console.log("Hello from JavaScript backend!");
    x = 42;
    console.log("Test complete");
    y = 840;
    console.log("840");
    __hir_tmp_0 = (_template + ("" + " This should become some HTML in the HTML build system hopefully soon"));
    _template = __hir_tmp_0;
    return _template;
}

test_js_output_bst_start();
