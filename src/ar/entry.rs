use chrono::{DateTime, Utc};
use std::{collections::HashMap, io::{Read, Seek, SeekFrom, Write}, path};

/// The `CompressMethod` represents all ways that a [File]'s data can be compressed in the archive
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressMethod {
    /// DEFLATE compression algorithm
    Deflate,
    /// Glib DEFLATE compression algorithm
    Gzip,
    /// No compression at all
    None,
}

impl std::str::FromStr for CompressMethod {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deflate" => Ok(Self::Deflate),
            "gzip" => Ok(Self::Gzip),
            "none" => Ok(Self::None),
            other => Err(other.to_owned()),
        }
    }
}

impl ToString for CompressMethod {
    fn to_string(&self) -> String {
        match self {
            Self::Deflate => "deflate".into(),
            Self::Gzip => "gzip".into(),
            Self::None => "none".into(),
        }
    }
}

/// Metadata values that can be applied to all entries, like notes and last updated times
#[derive(Debug, Default, Clone)]
pub struct Meta {
    /// The note that a user left for this entry
    pub note: Option<String>,

    /// If this entry has been used / watched
    pub used: bool,

    /// The name of this entry
    pub name: String,

    /// The last time that this entry was updated
    pub last_update: Option<DateTime<Utc>>,
}

/// The `File` entry is used in the [File](Entry::File) entry variant and contains all possible metadata like notes,
#[derive(Debug, Clone)]
pub struct File {
    /// The metadata of this file entry
    pub meta: Meta,

    /// The compression method of this file
    pub compression: CompressMethod,

    /// The offset into the file that this file's data is
    pub off: u64,

    /// The size of this file in the file data section in bytes
    pub size: u32,
}

impl File {
    pub fn write_data<W: Write, R: Read + Seek>(&self, off: &mut u64, writer: &mut W, reader: &mut R) -> std::io::Result<Entry> {
        reader.seek(SeekFrom::Start(self.off))?; 
        let mut buf = vec![0u8 ; self.size as usize];
        reader.read_exact(&mut buf)?;
        std::io::copy(&mut buf.as_slice(), writer)?; //Copy file data to the writer
        
        let ret = Entry::File(
            Self {
                meta: self.meta.clone(),
                off: *off,
                size: self.size,
                compression: self.compression,
            }
        );
        *off += self.size as u64;
        Ok(ret)
    }
}


/// The `Dir` entry is used in the [Dir](Entry::Dir) entry variant and contains [File]s and [Dir]s in it
#[derive(Debug, Default, Clone)]
pub struct Dir {
    /// Metadata of this directory
    pub meta: Meta,

    /// The contained data of this `Dir`
    pub data: HashMap<String, Entry>,
}

impl Dir {
    pub fn write_data<W: Write, R: Read + Seek>(&self, off: &mut u64, writer: &mut W, reader: &mut R) -> std::io::Result<Entry> {
        Ok(Entry::Dir(Self {
            meta: self.meta.clone(),
            data: self.data.iter().map(|(key, val)| match val.write_file_data(off, writer, reader) {
                Ok(val) => Ok((key.clone(), val)),
                Err(e) => Err(e)
            } ).collect::<Result<HashMap<String, Entry>, _>>()?
        }))
    }

    /// Add an entry to the directory using its name
    pub fn add_entry(&mut self, entry: Entry) {
        self.data.insert(entry.name(), entry);
    }

    fn get_entry<'a>(
        &self,
        mut paths: impl Iterator<Item = path::Component<'a>>,
    ) -> Option<&Entry> {
        match paths.next() {
            //If there is still more paths to follow, make sure we are a directory and get the nested entries
            Some(path) => self
                .data
                .get(path.as_os_str().to_str().unwrap())?
                .get_entry(paths),
            //If this is the end of the path, then return self
            None => None,
        }
    }

    fn get_entry_mut<'a>(
        &mut self,
        mut paths: impl Iterator<Item = path::Component<'a>>,
    ) -> Option<&mut Entry> {
        match paths.next() {
            //If there is still more paths to follow, make sure we are a directory and get the nested entries
            Some(path) => self
                .data
                .get_mut(path.as_os_str().to_str().unwrap())?
                .get_entry_mut(paths),
            //If this is the end of the path, then return self
            None => None,
        }
    }


    #[inline]
    pub fn entry<'a>(&self, paths: impl AsRef<path::Path>) -> Option<&Entry> {
        self.get_entry(paths.as_ref().components())
    }
    #[inline]
    pub fn entry_mut<'a>(&mut self, paths: impl AsRef<path::Path>) -> Option<&mut Entry> {
        self.get_entry_mut(paths.as_ref().components())
    }

}

/// The `Entry` struct represents one entry in the bar archive. It is the end result of parsing a
/// bar file and contains methods to both deserialize and serialize a bar file
#[derive(Debug, Clone)]
pub enum Entry {
    /// A file with offsets, name, etc.
    File(File),

    /// A directory that contains files
    Dir(Dir),
}

impl Entry {
    /// If this `Entry` is a [Dir], then get an entry from it, if it exists.
    /// This works with nested paths, for instance:
    ///
    /// if a directory has a nested directory 'test' that contains a file
    /// a.txt, then calling `get_entry` on the top directory with the path
    /// 'test/a.txt' will return `Some` with the file's data
    #[inline]
    pub fn entry<'a>(&self, paths: impl AsRef<path::Path>) -> Option<&Entry> {
        self.get_entry(paths.as_ref().components())
    }

    /// Get a mutable reference to an `Entry`.
    /// For more information, see [get_entry](Entry::get_entry)
    #[inline]
    pub fn entry_mut<'a>(&mut self, paths: impl AsRef<path::Path>) -> Option<&mut Entry> {
        self.get_entry_mut(paths.as_ref().components())
    }

    /// Write file data to a writer, returning new headers with updated offsets
    pub fn write_file_data<W: Write, R: Read + Seek>(&self, off: &mut u64, writer: &mut W, reader: &mut R) -> std::io::Result<Entry> {
        match self {
            Self::Dir(dir) => dir.write_data(off, writer, reader),
            Self::File(file ) => file.write_data(off, writer, reader),
        }
    }

    pub const fn as_dir(&self) -> Option<&Dir> {
        match self {
            Self::Dir(dir) => Some(dir),
            _ => None
        }
    }

    pub fn as_dir_mut(&mut self) -> Option<&mut Dir> {
        match self {
            Self::Dir(dir) => Some(dir),
            _ => None
        }
    }

    pub const fn as_file(&self) -> Option<&File> {
        match self {
            Self::File(file) => Some(file),
            _ => None
        }
    }
        
    /// Get the name of this file or directory
    #[inline(always)]
    pub fn name(&self) -> String {
        match self {
            Self::Dir(dir) => dir.meta.name.clone(),
            Self::File(file) => file.meta.name.clone(),
        }
    }

    /// Get the metadata of this entry
    pub const fn meta(&self) -> &Meta {
        match self {
            Self::Dir(ref dir) => &dir.meta,
            Self::File(ref file) => &file.meta,
        }
    }

    fn get_entry<'a>(
        &self,
        mut paths: impl Iterator<Item = path::Component<'a>>,
    ) -> Option<&Entry> {
        match paths.next() {
            //If there is still more paths to follow, make sure we are a directory and get the nested entries
            Some(path) => match self {
                Self::Dir(dir) => dir
                    .data
                    .get(path.as_os_str().to_str().unwrap())?
                    .get_entry(paths),
                Self::File(_) => None,
            },
            //If this is the end of the path, then return self
            None => Some(self),
        }
    }

    fn get_entry_mut<'a>(
        &mut self,
        mut paths: impl Iterator<Item = path::Component<'a>>,
    ) -> Option<&mut Entry> {
        match paths.next() {
            //If there is still more paths to follow, make sure we are a directory and get the nested entries
            Some(path) => match self {
                Self::Dir(dir) => dir
                    .data
                    .get_mut(path.as_os_str().to_str().unwrap())?
                    .get_entry_mut(paths),
                Self::File(_) => None,
            },
            //If this is the end of the path, then return self
            None => Some(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_entry() {
        let mut root = Dir {
            ..Default::default()
        };
        root.data.insert(
            "test".into(),
            Entry::Dir(Dir {
                meta: Meta {
                    name: "test".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
        let mut root = Entry::Dir(root);
        match root.entry_mut("test").unwrap() {
            Entry::Dir(dir) => dir.data.insert(
                "test.txt".into(),
                Entry::File(File {
                    meta: Meta {
                        name: "test.txt".into(),
                        ..Default::default()
                    },
                    compression: CompressMethod::None,
                    off: 0,
                    size: 0,
                }),
            ),
            _ => panic!("Not a directory!"),
        };
        let _ = root.entry("test/test.txt").unwrap();
    }
}
