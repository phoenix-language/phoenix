//! Call frames and operand stack.

/// MVP runtime scalar: one signed 64-bit integer slot (`s64` in Phoenix types).
///
/// Rust uses `i64` as the host representation; language docs and diagnostics use `s32` / `s64` / `u32`, not `i32` / `i64`.
pub type Value = i64;

/// One activation record.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Function id in the module.
    pub function_id: u32,
    /// Byte offset into the function's code slice.
    pub pc: u32,
    /// Local slots (parameters occupy `0..arity`).
    pub locals: Vec<Value>,
}

/// Operand stack + call stack.
#[derive(Debug, Default)]
pub struct Machine {
    /// Evaluation stack.
    pub stack: Vec<Value>,
    /// Innermost frame is the current function.
    pub frames: Vec<Frame>,
}

impl Machine {
    /// Pushes a new frame with `local_count` zeroed locals.
    pub fn push_frame(&mut self, function_id: u32, local_count: u16) {
        let n = usize::from(local_count);
        self.frames.push(Frame {
            function_id,
            pc: 0,
            locals: vec![0; n],
        });
    }

    /// Pops the current frame.
    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.frames.pop()
    }
}
