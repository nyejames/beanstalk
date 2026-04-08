//! Heap abstraction for interpreter-owned runtime objects.
//!
//! WHAT: provides a backend-local object store and opaque handles.
//! WHY: the runtime must depend on a Beanstalk heap layer rather than leaking GC-library types.

use crate::backends::rust_interpreter::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HeapHandle(pub u32);

#[derive(Debug, Clone)]
pub(crate) struct Heap {
    objects: Vec<HeapObject>,
}

impl Heap {
    pub(crate) fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    pub(crate) fn allocate(&mut self, object: HeapObject) -> HeapHandle {
        let handle = HeapHandle(self.objects.len() as u32);
        self.objects.push(object);
        handle
    }

    pub(crate) fn get(&self, handle: HeapHandle) -> Option<&HeapObject> {
        self.objects.get(handle.0 as usize)
    }

    pub(crate) fn get_mut(&mut self, handle: HeapHandle) -> Option<&mut HeapObject> {
        self.objects.get_mut(handle.0 as usize)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum HeapObject {
    String(StringObject),
    StringBuilder(StringBuilderObject),
    Record(RecordObject),
}

#[derive(Debug, Clone)]
pub(crate) struct StringObject {
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StringBuilderObject {
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RecordObject {
    pub fields: Vec<Value>,
}
