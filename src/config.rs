#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub max_buffer_size: usize,
    pub max_message_size: usize,
    pub initial_capacity: usize,
    pub strict_validation: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub const fn new() -> Self {
        Self {
            max_buffer_size: 1024 * 1024 * 1024, // 1 GB
            max_message_size: 64 * 1024,         // 64 KB
            initial_capacity: 2 * 1024 * 1024,   // 2 MB
            strict_validation: false,
        }
    }

    pub const fn with_max_buffer_size(mut self, size: usize) -> Self {
        self.max_buffer_size = size;
        self
    }

    pub const fn with_max_buffer_size_mb(mut self, size_mb: usize) -> Self {
        self.max_buffer_size = size_mb * 1024 * 1024;
        self
    }

    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    pub const fn with_initial_capacity(mut self, capacity: usize) -> Self {
        self.initial_capacity = capacity;
        self
    }

    pub const fn with_strict_validation(mut self, strict: bool) -> Self {
        self.strict_validation = strict;
        self
    }

    pub fn from_size_mb(size_mb: usize) -> Self {
        Self::new().with_max_buffer_size_mb(size_mb)
    }
}
