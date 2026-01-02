use crate::syntax::{self, ContextReference}; /* ContextReference is just struct (u16, u16) */
use std::collections::HashMap;

pub struct Stack {
    frames: Vec<Frame>,
    map: HashMap<(ContextReference, usize), usize>, /* maps frame to id, for searching */
}

#[derive(Clone, Copy)]
pub struct Frame {
    context: ContextReference,
    parent: usize,
}

impl Stack {
    pub fn new() -> Self {
        Self {
            frames: vec![Frame {
                context: ContextReference(0, 0),
                parent: 0,
            }],
            map: HashMap::<(ContextReference, usize), usize>::new(),
        }
    }

    pub fn empty(self: &Self) -> usize {
        0
    }

    pub fn top(&self, id: usize) -> Option<ContextReference> {
        if id == 0 {
            None
        } else {
            Some(self.frames[id].context)
        }
    }

    pub fn push(self: &mut Self, context: ContextReference, parent: usize) -> usize {
        let key = (context, parent);
        if let Some(&id) = self.map.get(&key) {
            id
        } else {
            let id = self.frames.len();
            self.frames.push(Frame { context, parent });
            self.map.insert(key, id);
            id
        }
    }

    pub fn pop(self: &Self, id: usize) -> usize {
        debug_assert!(id < self.frames.len());
        self.frames[id].parent
    }

    pub fn set(self: &mut Self, id: usize, to: ContextReference) -> usize {
        let p = self.pop(id);
        self.push(to, p)
    }
}
