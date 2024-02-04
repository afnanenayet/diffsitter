use std::{io::Write, ops::Deref};

/// Implements the [std::io:Write] trait for strings.
///
/// This is used to test various utilities that write to a generic writer so we can test the
/// expected terminal output in unit tests.
///
/// This also derefs to a string so can be used basically as a drop in string.
pub(crate) struct StringWriter {
    internal_buffer: String,
}

impl StringWriter {
    pub fn new() -> Self {
        Self {
            internal_buffer: String::new(),
        }
    }

    /// Consume the writer and return the internal string.
    pub fn consume(self) -> String {
        self.internal_buffer
    }
}

impl Write for StringWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8(buf.to_vec()).unwrap();
        self.internal_buffer += &s;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Deref for StringWriter {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.internal_buffer
    }
}
