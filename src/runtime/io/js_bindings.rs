// JavaScript/DOM bindings for web targets
//
// Provides comprehensive JS code generation for WASM integration,
// DOM manipulation, and web-specific functionality.

use crate::compiler::compiler_errors::CompileError;
use crate::runtime::io::io::{IoInterface, IoConfig};

pub struct JsBindingsIoBackend {
    config: IoConfig,
}

impl JsBindingsIoBackend {
    pub fn new(config: IoConfig) -> Self {
        Self { config }
    }
}

impl IoInterface for JsBindingsIoBackend {
    fn print(&self, message: &str) -> Result<(), CompileError> {
        // In a real web environment, this would call console.log via JS bindings
        println!("JS Console: {}", message);
        Ok(())
    }
    
    fn read_input(&self) -> Result<String, CompileError> {
        // In a web environment, this might use prompt() or read from DOM elements
        Err(CompileError::compiler_error("Input reading not implemented for JS bindings"))
    }
    
    fn write_file(&self, _path: &str, _content: &str) -> Result<(), CompileError> {
        // Web environments typically can't write arbitrary files
        Err(CompileError::compiler_error("File writing not supported in web environment"))
    }
    
    fn read_file(&self, _path: &str) -> Result<String, CompileError> {
        // Web environments might fetch files via HTTP
        Err(CompileError::compiler_error("File reading not implemented for JS bindings"))
    }
    
}

/// Generates comprehensive JavaScript code for WASM integration and DOM manipulation
pub struct JsBindingsGenerator {
    wasm_module_name: String,
    include_dom_functions: bool,
    include_dev_features: bool,
}

impl JsBindingsGenerator {
    pub fn new(wasm_module_name: String) -> Self {
        Self {
            wasm_module_name,
            include_dom_functions: true,
            include_dev_features: false,
        }
    }

    pub fn with_dom_functions(mut self, include: bool) -> Self {
        self.include_dom_functions = include;
        self
    }

    pub fn with_dev_features(mut self, include: bool) -> Self {
        self.include_dev_features = include;
        self
    }

    /// Generate the complete JavaScript code block for WASM integration
    pub fn generate_js_bindings(&self) -> String {
        let mut js_code = String::new();

        // Add WASM module class
        js_code.push_str(&self.generate_wasm_module_class());

        // Add DOM manipulation functions if requested
        if self.include_dom_functions {
            js_code.push_str(&self.generate_dom_manipulation_functions());
        }

        // Add development features if requested
        if self.include_dev_features {
            js_code.push_str(&self.generate_dev_features());
        }

        // Add initialization code
        js_code.push_str(&self.generate_initialization_code());

        js_code
    }

    fn generate_wasm_module_class(&self) -> String {
        format!(
            r#"
// Beanstalk WASM Module Integration
class BeanstalkModule {{
    constructor() {{
        this.instance = null;
        this.memory = null;
        this.initialized = false;
    }}

    async init() {{
        if (this.initialized) {{
            return;
        }}

        try {{
            const wasmModule = await WebAssembly.instantiateStreaming(
                fetch('./{}.wasm'),
                this.getImports()
            );
            
            this.instance = wasmModule.instance;
            this.memory = this.instance.exports.memory;
            this.initialized = true;
            
            // Call initialization if available
            if (this.instance.exports._start) {{
                this.instance.exports._start();
            }}
            
            console.log('Beanstalk WASM module initialized successfully');
        }} catch (error) {{
            console.error('Failed to initialize Beanstalk WASM module:', error);
            throw error;
        }}
    }}

    getImports() {{
        return {{
            // Beanstalk IO module - provides console and basic I/O
            beanstalk_io: {{
                print: (ptr, len) => {{
                    const bytes = new Uint8Array(this.memory.buffer, ptr, len);
                    const text = new TextDecoder().decode(bytes);
                    console.log(text);
                }},
                
                read_input: (bufferPtr) => {{
                    // In web environment, could use prompt() or read from DOM
                    console.warn('read_input not implemented in web environment');
                    return 0;
                }},
                
                write_file: (pathPtr, pathLen, contentPtr, contentLen) => {{
                    // Web environments typically can't write arbitrary files
                    console.warn('write_file not supported in web environment');
                    return -1; // Error
                }},
                
                read_file: (pathPtr, pathLen, bufferPtr) => {{
                    // Could fetch files via HTTP in web environment
                    console.warn('read_file not implemented in web environment');
                    return 0;
                }}
            }},
            
            // Beanstalk environment module
            beanstalk_env: {{
                get_env: (keyPtr, keyLen, bufferPtr) => {{
                    // Web environments don't have traditional environment variables
                    return -1; // Not found
                }},
                
                set_env: (keyPtr, keyLen, valuePtr, valueLen) => {{
        // Web environments don't have traditional environment variables
                    return 0; // Success (no-op)
                }}
            }},
            
            // Beanstalk DOM module - provides DOM manipulation functions
            beanstalk_dom: {{
                // Element selection
                get_element_by_id: (idPtr, idLen) => {{
                    const id = this.readString(idPtr, idLen);
                    const element = document.getElementById(id);
                    return element ? this.getElementPointer(element) : 0;
                }},
                
                get_elements_by_class: (classPtr, classLen) => {{
                    const className = this.readString(classPtr, classLen);
                    const elements = document.getElementsByClassName(className);
                    return this.createElementArray(elements);
                }},
                
                // DOM manipulation
                set_inner_html: (elementPtr, contentPtr, contentLen) => {{
                    const element = this.getElementFromPointer(elementPtr);
                    const content = this.readString(contentPtr, contentLen);
                    if (element) {{
                        element.innerHTML = content;
                        return 1; // Success
                    }}
                    return 0; // Error
                }},
                
                append_child: (parentPtr, childPtr) => {{
                    const parent = this.getElementFromPointer(parentPtr);
                    const child = this.getElementFromPointer(childPtr);
                    if (parent && child) {{
                        parent.appendChild(child);
                        return 1; // Success
                    }}
                    return 0; // Error
                }},
                
                remove_child: (parentPtr, childPtr) => {{
                    const parent = this.getElementFromPointer(parentPtr);
                    const child = this.getElementFromPointer(childPtr);
                    if (parent && child && parent.contains(child)) {{
                        parent.removeChild(child);
                        return 1; // Success
                    }}
                    return 0; // Error
                }},
                
                create_element: (tagPtr, tagLen) => {{
                    const tagName = this.readString(tagPtr, tagLen);
                    const element = document.createElement(tagName);
                    return this.getElementPointer(element);
                }},
                
                set_attribute: (elementPtr, namePtr, nameLen, valuePtr, valueLen) => {{
                    const element = this.getElementFromPointer(elementPtr);
                    const name = this.readString(namePtr, nameLen);
                    const value = this.readString(valuePtr, valueLen);
                    if (element) {{
                        element.setAttribute(name, value);
                        return 1; // Success
                    }}
                    return 0; // Error
                }},
                
                get_attribute: (elementPtr, namePtr, nameLen, bufferPtr) => {{
                    const element = this.getElementFromPointer(elementPtr);
                    const name = this.readString(namePtr, nameLen);
                    if (element) {{
                        const value = element.getAttribute(name) || '';
                        this.writeString(value, bufferPtr);
                        return value.length;
                    }}
                    return 0; // Error
                }}
            }}
            
            // Note: beanstalk_sys module (exit) not included for web safety
        }};
    }}

    // Call exported functions
    call(functionName, ...args) {{
        if (this.instance && this.instance.exports[functionName]) {{
            return this.instance.exports[functionName](...args);
        }}
        throw new Error(`Function ${{functionName}} not found`);
    }}
    
    // Helper to read string from WASM memory
    readString(ptr, len) {{
        const bytes = new Uint8Array(this.memory.buffer, ptr, len);
        return new TextDecoder().decode(bytes);
    }}
    
    // Helper to write string to WASM memory
    writeString(str, ptr) {{
        const bytes = new TextEncoder().encode(str);
        const memory = new Uint8Array(this.memory.buffer);
        memory.set(bytes, ptr);
        return bytes.length;
    }}

    // Element pointer management for DOM operations
    elementPointers = new Map();
    nextElementPointer = 1;

    getElementPointer(element) {{
        const pointer = this.nextElementPointer++;
        this.elementPointers.set(pointer, element);
        return pointer;
    }}

    getElementFromPointer(pointer) {{
        return this.elementPointers.get(pointer) || null;
    }}

    createElementArray(elements) {{
        const pointers = [];
        for (let i = 0; i < elements.length; i++) {{
            pointers.push(this.getElementPointer(elements[i]));
        }}
        // Store array in memory and return pointer to it
        // This is a simplified version - in practice you'd need proper array handling
        return this.getElementPointer(elements);
    }}
}}
"#,
            self.wasm_module_name
        )
    }

    fn generate_dom_manipulation_functions<'a>(&self) -> &'a str {
        r#"
// DOM Manipulation Functions
// These functions provide a higher-level interface for DOM operations

function uInnerHTML(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ');
    }
    
    for (let i = 0; i < elements.length; i++) {
        elements[i].innerHTML = update;
    }
}

function uAppendChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    if (Array.isArray(update)) {
        update = update.join(' ');
    }
    
    for (let i = 0; i < elements.length; i++) {
        if (typeof update === 'string') {
            elements[i].insertAdjacentHTML('beforeend', update);
        } else {
            elements[i].appendChild(update);
        }
    }
}

function uRemoveChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    for (let i = 0; i < elements.length; i++) {
        if (typeof update === 'string') {
            // Remove by selector
            const children = elements[i].querySelectorAll(update);
            children.forEach(child => child.remove());
        } else {
            elements[i].removeChild(update);
        }
    }
}

function uReplaceChild(id, update) {
    const elements = document.getElementsByClassName(id);
    
    for (let i = 0; i < elements.length; i++) {
        if (typeof update === 'string') {
            elements[i].innerHTML = update;
        } else {
            // For element replacement, you'd need more complex logic
            elements[i].replaceWith(update);
        }
    }
}

// Additional DOM utility functions
function createElement(tagName, attributes = {}, content = '') {
    const element = document.createElement(tagName);
    
    // Set attributes
    for (const [key, value] of Object.entries(attributes)) {
        element.setAttribute(key, value);
    }
    
    // Set content
    if (content) {
        element.innerHTML = content;
    }
    
    return element;
}

function addEventListener(selector, event, handler) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        element.addEventListener(event, handler);
    });
}

function removeEventListener(selector, event, handler) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        element.removeEventListener(event, handler);
    });
}

function setStyle(selector, styles) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        for (const [property, value] of Object.entries(styles)) {
            element.style[property] = value;
        }
    });
}

function addClass(selector, className) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        element.classList.add(className);
    });
}

function removeClass(selector, className) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        element.classList.remove(className);
    });
}

function toggleClass(selector, className) {
    const elements = document.querySelectorAll(selector);
    elements.forEach(element => {
        element.classList.toggle(className);
    });
}
"#
    }

    fn generate_dev_features<'a>(&self) -> &'a str {
        r#"
// Development Features
function checkIfFileChanged() {
    const currentUrl = window.location.pathname;
    const requestUrl = `/check?page=${currentUrl}`;

    fetch(requestUrl, { method: 'HEAD' })
        .then(response => {
            if (response.status === 205) {
                location.reload();
            }
        })
        .catch(console.error);
}

// Auto-reload in development
if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
    setInterval(() => checkIfFileChanged(), 600);
}

// Development console helpers
window.beanstalkDev = {
    module: null,
    
    init(module) {
        this.module = module;
        console.log('Beanstalk development tools initialized');
    },
    
    callFunction(name, ...args) {
        if (this.module) {
            return this.module.call(name, ...args);
        }
        console.error('Beanstalk module not initialized');
    },
    
    inspectMemory(ptr, len) {
        if (this.module && this.module.memory) {
            const bytes = new Uint8Array(this.module.memory.buffer, ptr, len);
            return new TextDecoder().decode(bytes);
        }
        return null;
    }
};
"#
    }

    fn generate_initialization_code<'a>(&self) -> &'a str {
        r#"
// Auto-initialize when DOM is ready
document.addEventListener('DOMContentLoaded', async () => {
    try {
        window.beanstalk = new BeanstalkModule();
        await window.beanstalk.init();
        
        // Initialize development tools if available
        if (window.beanstalkDev) {
            window.beanstalkDev.init(window.beanstalk);
        }
        
        // Dispatch custom event for other scripts
        window.dispatchEvent(new CustomEvent('beanstalkReady', {
            detail: { module: window.beanstalk }
        }));
        
    } catch (error) {
        console.error('Failed to initialize Beanstalk:', error);
    }
});
"#
    }
}