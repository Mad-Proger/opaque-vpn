use std::ptr::read;

use tokio::io::{ReadHalf, WriteHalf};

#[derive(Default)]
pub struct Unsplit<T> {
    read_half: Option<ReadHalf<T>>,
    write_half: Option<WriteHalf<T>>,
}

pub enum UnsplitError {
    Occupied,
    Incompatible,
}

impl std::fmt::Debug for UnsplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnsplitError::Occupied => "the half is already stored",
            UnsplitError::Incompatible => "the other half is not from the same pair",
        })
    }
}

impl std::fmt::Display for UnsplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Debug>::fmt(self, f)
    }
}

impl std::error::Error for UnsplitError {}

impl<T: Unpin> Unsplit<T> {
    pub fn save_write_half(&mut self, write_half: WriteHalf<T>) -> Result<(), UnsplitError> {
        if self.write_half.is_some() {
            return Err(UnsplitError::Occupied);
        }
        if self
            .read_half
            .as_ref()
            .map(|read_half| !read_half.is_pair_of(&write_half))
            .unwrap_or(false)
        {
            return Err(UnsplitError::Incompatible);
        }
        self.write_half = write_half.into();
        Ok(())
    }

    pub fn save_read_half(&mut self, read_half: ReadHalf<T>) -> Result<(), UnsplitError> {
        if self.read_half.is_some() {
            return Err(UnsplitError::Occupied);
        }
        if self
            .write_half
            .as_ref()
            .map(|write_half| !write_half.is_pair_of(&read_half))
            .unwrap_or(false)
        {
            return Err(UnsplitError::Incompatible);
        }
        self.read_half = read_half.into();
        Ok(())
    }

    pub fn unsplit(mut self) -> Option<T> {
        self.read_half
            .take()
            .zip(self.write_half.take())
            .map(|(read_half, write_half)| read_half.unsplit(write_half))
    }
}
