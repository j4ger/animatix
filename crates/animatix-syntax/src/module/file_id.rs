use super::FileId;

impl FileId {
    /// Create a new `FileId` from a raw index.
    pub fn new(id: u32) -> Self {
        FileId(id)
    }
    /// Return the underlying index.
    pub fn index(&self) -> u32 {
        self.0
    }
}
