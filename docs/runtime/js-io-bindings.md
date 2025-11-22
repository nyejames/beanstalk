# JavaScript IO Bindings for Beanstalk

## Overview

Beanstalk's JavaScript IO bindings provide the runtime implementation for the `io()` function when executing WASM modules in web browsers or Node.js environments. The bindings automatically handle newline appending to provide consistent behavior across all platforms.

## IO Function Signature

### WASM Import

The `io()` function is imported from the `beanstalk_io` module:

```wasm
(import "beanstalk_io" "io" (func $io (param i32 i32)))
```

### JavaScript Implementation

```javascript
beanstalk_io: {
    io: (ptr, len) => {
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        const text = new TextDecoder().decode(bytes);
        console.log(text);
    }
}
```

## Parameters

- **ptr** (i32): Pointer to the string data in WASM linear memory
- **len** (i32): Length of the string in bytes

## Behavior

### Automatic Newline Appending

The `io()` function automatically appends a newline character to all output. This is achieved through the use of `console.log()`, which adds a newline by default.

**Beanstalk code:**
```beanstalk
io("Hello, World!")
io("Second line")
```

**Output:**
```
Hello, World!
Second line
```

### UTF-8 Decoding

The JavaScript binding uses `TextDecoder` to properly decode UTF-8 strings from WASM memory:

```javascript
const bytes = new Uint8Array(memory.buffer, ptr, len);
const text = new TextDecoder().decode(bytes);
```

This ensures correct handling of:
- ASCII characters
- Unicode characters
- Multi-byte UTF-8 sequences
- Emoji and special symbols

### Memory Safety

The binding reads directly from WASM linear memory using the provided pointer and length:

```javascript
const bytes = new Uint8Array(memory.buffer, ptr, len);
```

**Safety guarantees:**
- Bounds checking is performed by the WASM runtime
- Invalid pointers or lengths will cause WASM traps
- No buffer overflows possible due to WASM memory isolation

## Integration Example

### Complete WASM Module Setup

```javascript
// Beanstalk WASM Module Integration
class BeanstalkModule {
    constructor() {
        this.instance = null;
        this.memory = null;
        this.initialized = false;
    }

    async init() {
        if (this.initialized) {
            return;
        }

        try {
            const wasmModule = await WebAssembly.instantiateStreaming(
                fetch('./program.wasm'),
                this.getImports()
            );
            
            this.instance = wasmModule.instance;
            this.memory = this.instance.exports.memory;
            this.initialized = true;
            
            // Call initialization if available
            if (this.instance.exports._start) {
                this.instance.exports._start();
            }
            
            console.log('Beanstalk WASM module initialized successfully');
        } catch (error) {
            console.error('Failed to initialize Beanstalk WASM module:', error);
            throw error;
        }
    }

    getImports() {
        return {
            beanstalk_io: {
                io: (ptr, len) => {
                    const bytes = new Uint8Array(this.memory.buffer, ptr, len);
                    const text = new TextDecoder().decode(bytes);
                    console.log(text);
                }
            }
        };
    }
}

// Auto-initialize when DOM is ready
document.addEventListener('DOMContentLoaded', async () => {
    try {
        window.beanstalk = new BeanstalkModule();
        await window.beanstalk.init();
    } catch (error) {
        console.error('Failed to initialize Beanstalk:', error);
    }
});
```

### Node.js Integration

```javascript
const fs = require('fs');
const { TextDecoder } = require('util');

async function runBeanstalk(wasmPath) {
    const wasmBytes = fs.readFileSync(wasmPath);
    
    const imports = {
        beanstalk_io: {
            io: (ptr, len) => {
                const memory = wasmInstance.exports.memory;
                const bytes = new Uint8Array(memory.buffer, ptr, len);
                const text = new TextDecoder().decode(bytes);
                console.log(text);
            }
        }
    };
    
    const wasmModule = await WebAssembly.instantiate(wasmBytes, imports);
    const wasmInstance = wasmModule.instance;
    
    // Run the program
    if (wasmInstance.exports._start) {
        wasmInstance.exports._start();
    }
}

runBeanstalk('./program.wasm');
```

## Comparison with Other IO Methods

### io() vs template_output()

| Feature | io() | template_output() |
|---------|------|-------------------|
| Newline | Automatic | Manual |
| Use case | Line-based output | Template rendering |
| Beanstalk syntax | `io("text")` | `[:text]` (deprecated) |

### Future: io() vs Io.write()

The `io()` function is designed for convenient line-based output. In the future, `Io.write()` will provide output without automatic newlines:

```beanstalk
-- Current: io() with automatic newline
io("Hello")  -- Outputs: "Hello\n"

-- Future: Io.write() without newline
Io.write("Hello")  -- Outputs: "Hello"
Io.write(" ")
Io.write("World")  -- Outputs: "Hello World"
```

## Error Handling

### Invalid UTF-8

If the WASM memory contains invalid UTF-8 data, `TextDecoder` will replace invalid sequences with the Unicode replacement character (�):

```javascript
const text = new TextDecoder().decode(bytes);
// Invalid UTF-8 → "Hello � World"
```

### Memory Access Errors

Memory access errors are handled by the WASM runtime:

```javascript
// If ptr or len are invalid, WASM will trap before reaching JS
const bytes = new Uint8Array(memory.buffer, ptr, len);
```

### Console Availability

The binding assumes `console.log` is available. In environments without console:

```javascript
beanstalk_io: {
    io: (ptr, len) => {
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        const text = new TextDecoder().decode(bytes);
        
        // Fallback for environments without console
        if (typeof console !== 'undefined' && console.log) {
            console.log(text);
        } else {
            // Alternative output method
            document.body.appendChild(document.createTextNode(text + '\n'));
        }
    }
}
```

## Performance Considerations

### Memory Copying

The binding creates a copy of the string data from WASM memory:

```javascript
const bytes = new Uint8Array(memory.buffer, ptr, len);
```

This is necessary because:
- WASM memory can be resized, invalidating views
- JavaScript strings are immutable
- The copy is minimal (only the string length)

### TextDecoder Reuse

For better performance with many `io()` calls, reuse a TextDecoder instance:

```javascript
const decoder = new TextDecoder();

beanstalk_io: {
    io: (ptr, len) => {
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        const text = decoder.decode(bytes);
        console.log(text);
    }
}
```

### Batching Output

For high-frequency output, consider batching:

```javascript
let outputBuffer = [];
let flushTimeout = null;

beanstalk_io: {
    io: (ptr, len) => {
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        const text = decoder.decode(bytes);
        
        outputBuffer.push(text);
        
        // Flush after 10ms of inactivity
        clearTimeout(flushTimeout);
        flushTimeout = setTimeout(() => {
            console.log(outputBuffer.join('\n'));
            outputBuffer = [];
        }, 10);
    }
}
```

## Testing

### Unit Testing

```javascript
describe('Beanstalk IO Binding', () => {
    let memory;
    let ioBinding;
    let output;
    
    beforeEach(() => {
        // Create mock WASM memory
        memory = new WebAssembly.Memory({ initial: 1 });
        output = [];
        
        // Create io binding that captures output
        ioBinding = (ptr, len) => {
            const bytes = new Uint8Array(memory.buffer, ptr, len);
            const text = new TextDecoder().decode(bytes);
            output.push(text);
        };
    });
    
    test('outputs simple string', () => {
        // Write "Hello" to memory at offset 0
        const view = new Uint8Array(memory.buffer);
        const text = "Hello";
        for (let i = 0; i < text.length; i++) {
            view[i] = text.charCodeAt(i);
        }
        
        // Call io binding
        ioBinding(0, text.length);
        
        expect(output).toEqual(['Hello']);
    });
    
    test('handles UTF-8 correctly', () => {
        const view = new Uint8Array(memory.buffer);
        const text = "Hello 世界 🌍";
        const encoded = new TextEncoder().encode(text);
        view.set(encoded, 0);
        
        ioBinding(0, encoded.length);
        
        expect(output).toEqual(['Hello 世界 🌍']);
    });
});
```

### Integration Testing

```javascript
async function testBeanstalkIO() {
    // Compile simple Beanstalk program
    const beanstalkCode = `
        io("Line 1")
        io("Line 2")
        io("Line 3")
    `;
    
    // Compile to WASM (using Beanstalk compiler)
    const wasmBytes = await compileBeanstalk(beanstalkCode);
    
    // Capture output
    const output = [];
    const imports = {
        beanstalk_io: {
            io: (ptr, len) => {
                const memory = wasmInstance.exports.memory;
                const bytes = new Uint8Array(memory.buffer, ptr, len);
                const text = new TextDecoder().decode(bytes);
                output.push(text);
            }
        }
    };
    
    const wasmModule = await WebAssembly.instantiate(wasmBytes, imports);
    const wasmInstance = wasmModule.instance;
    
    if (wasmInstance.exports._start) {
        wasmInstance.exports._start();
    }
    
    // Verify output
    assert.deepEqual(output, ['Line 1', 'Line 2', 'Line 3']);
}
```

## Browser Compatibility

The JavaScript IO bindings are compatible with all modern browsers that support:

- WebAssembly (all major browsers since 2017)
- TextDecoder API (all major browsers since 2016)
- Uint8Array (all major browsers since 2011)

### Polyfills

For older environments, provide polyfills:

```javascript
// TextDecoder polyfill for very old browsers
if (typeof TextDecoder === 'undefined') {
    window.TextDecoder = function() {
        this.decode = function(bytes) {
            return String.fromCharCode.apply(null, bytes);
        };
    };
}
```

## Security Considerations

### Memory Isolation

WASM memory is isolated from JavaScript:
- Cannot access JavaScript heap
- Cannot access DOM directly
- All access goes through explicit imports

### XSS Prevention

The `io()` function outputs to console, not DOM:
- No HTML injection possible
- No script execution possible
- Safe for untrusted WASM modules

### Resource Limits

Consider rate limiting for untrusted code:

```javascript
let callCount = 0;
let lastReset = Date.now();

beanstalk_io: {
    io: (ptr, len) => {
        // Reset counter every second
        const now = Date.now();
        if (now - lastReset > 1000) {
            callCount = 0;
            lastReset = now;
        }
        
        // Limit to 1000 calls per second
        if (callCount++ > 1000) {
            console.warn('IO rate limit exceeded');
            return;
        }
        
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        const text = new TextDecoder().decode(bytes);
        console.log(text);
    }
}
```

## Summary

The JavaScript IO binding for Beanstalk's `io()` function provides:

✅ Automatic newline appending via `console.log()`  
✅ Proper UTF-8 decoding with `TextDecoder`  
✅ Memory safety through WASM isolation  
✅ Simple integration with web and Node.js  
✅ Consistent behavior across platforms  

The automatic newline behavior ensures that `io()` works like `println!` in Rust or `console.log()` in JavaScript, providing convenient line-based output for the most common use case.
