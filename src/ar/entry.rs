use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, aead::{AeadInPlace, NewAead}};
use flate2::write::{DeflateEncoder, GzEncoder};
use indicatif::ProgressBar;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read, Seek, SeekFrom, Write},
    path,
};

use super::BarResult;

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

/// The `CompressType` struct specifies both quality and mode of compression
#[derive(Debug, Clone, Copy)]
pub struct CompressType(pub flate2::Compression, pub CompressMethod);

impl std::str::FromStr for CompressType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.to_lowercase().as_str() == "none" {
            return Ok(Self(flate2::Compression::none(), CompressMethod::None));
        }

        let s = s.to_lowercase();
        let (quality, method) = s.split_once("-").ok_or_else(|| s.to_owned())?;
        let quality = match quality {
            "high" => flate2::Compression::best(),
            "fast" => flate2::Compression::fast(),
            "medium" => flate2::Compression::new(5),
            other => return Err(other.to_string()),
        };
        let method = match method {
            "gzip" => CompressMethod::Gzip,
            "deflate" => CompressMethod::Deflate,
            _ => return Err(s.to_owned()),
        };

        Ok(Self(quality, method))
    }
}

impl ToString for CompressType {
    fn to_string(&self) -> String {
        if self.1 == CompressMethod::None {
            return "none".into();
        }
        let quality = match self.0.level() {
            9 => "high",
            1 => "fast",
            5 => "medium",
            _ => unreachable!(),
        };

        let method = match self.1 {
            CompressMethod::Deflate => "deflate",
            CompressMethod::Gzip => "gzip",
            CompressMethod::None => unreachable!(),
        };

        quality.to_owned() + "-" + method
    }
}

/// Metadata values that can be applied to all entries, like notes and if this entry has been used / watched before
#[derive(Debug, Default, Clone)]
pub struct Meta {
    /// The note that a user left for this entry
    pub note: Option<String>,

    /// If this entry has been used / watched
    pub used: bool,

    /// The name of this entry
    pub name: String,
}

/// The `EncryptType` enum is stored in the [File] struct and specifies what kind of encryption + nonce if any
/// is present for the file
#[derive(Clone, Debug)]
pub enum EncryptType {
    /// ChaCha20 with nonce bytes
    ChaCha20(Nonce),

    /// No encryption
    None,
}

impl Default for EncryptType {
    fn default() -> Self {
        Self::None
    }
}

/// The `File` entry is used in the [File](Entry::File) entry variant and contains all possible metadata like notes,
#[derive(Debug, Clone)]
pub struct File {
    /// The metadata of this file entry
    pub meta: RefCell<Meta>,

    /// The compression method of this file
    pub(crate) compression: CompressType,

    /// The offset into the file that this file's data is
    pub(crate) off: u64,

    /// The size of this file in the file data section in bytes
    pub(crate) size: u32,

    /// The encryption method (if any) that this file is encrypted with
    pub(crate) enc: EncryptType,
}

impl File {
    pub const fn compression(&self) -> &CompressType {
        &self.compression
    }

    /// Write this `File`s data to a writer, compressing / encrypting bytes as needed
    pub fn write_data<W: Write, R: Read + Seek>(
        &self,
        off: &mut u64,
        writer: &mut W,
        reader: &mut R,
        prog: &ProgressBar,
    ) -> std::io::Result<Entry> {
        prog.set_message(format!("Saving file {}", self.meta.borrow().name));

        let this_prog = match prog.is_hidden() {
            false => ProgressBar::new(0).with_style(
                indicatif::ProgressStyle::default_bar()
                    .template("[{bar}] {bytes} {binary_bytes_per_sec} {msg}")
                    .progress_chars("=>-"),
            ),
            true => ProgressBar::hidden(),
        };

        reader.seek(SeekFrom::Start(self.off))?;
        let mut buf = vec![0u8; self.size as usize];

        this_prog.set_message("Reading file data from archive");
        this_prog.wrap_read(reader).read_exact(&mut buf)?;
        this_prog.reset();

        //Compress bytes if it is desired
        let bytes = match self.compression {
            CompressType(quality, CompressMethod::Deflate) => {
                let mut encoder = DeflateEncoder::new(Vec::new(), quality);

                this_prog.set_message("Compressing data with DEFLATE");
                this_prog
                    .wrap_write(&mut encoder)
                    .write_all(buf.as_slice())?;
                this_prog.reset();
                drop(buf);

                encoder.finish()?
            }
            CompressType(quality, CompressMethod::Gzip) => {
                let mut encoder = GzEncoder::new(Vec::new(), quality);

                this_prog.set_message("Compressing data with gzip");
                this_prog
                    .wrap_write(&mut encoder)
                    .write_all(buf.as_slice())?;
                this_prog.reset();

                drop(buf);
                encoder.finish()?
            }
            CompressType(_, CompressMethod::None) => buf,
        };

        let ret = Entry::File(Self {
            meta: self.meta.clone(),
            off: *off,
            size: bytes.len() as u32,
            compression: self.compression,
            enc: self.enc.clone(),
        });

        this_prog.set_message("Writing compressed bytes");
        std::io::copy(&mut bytes.as_slice(), &mut this_prog.wrap_write(writer))?; //Copy file data to the writer
        this_prog.finish_and_clear();

        *off += bytes.len() as u64;
        drop(bytes);
        Ok(ret)
    }

    pub const fn off(&self) -> u64 {
        self.off
    }

    pub const fn size(&self) -> u32 {
        self.size
    }

    /// Encrypt this file's data in place using the given key and nonce.
    /// This is a no-op if the file is already encrypted
    pub fn encrypt(&mut self, key: &Key, nonce: &Nonce, back: &mut (impl Write + Read + Seek)) -> BarResult<()> {
        if self.is_encrypted() {
            return Ok(())
        }

        self.enc = EncryptType::ChaCha20(nonce.clone());
        let cipher = ChaCha20Poly1305::new(key);
        back.seek(SeekFrom::Start(self.off))?;

        let mut data = vec![0u8 ; self.size as usize];
        back.read_exact(&mut data)?;

        cipher.encrypt_in_place(nonce, b"", &mut data)?;
        Ok(())
    }

    /// Check if this file's data is encrypted
    pub const fn is_encrypted(&self) -> bool {
        match self.enc {
            EncryptType::ChaCha20(_) => true,
            _ => false
        }
    }
}

/// The `Dir` entry is used in the [Dir](Entry::Dir) entry variant and contains [File]s and [Dir]s in it
#[derive(Debug, Default, Clone)]
pub struct Dir {
    /// Metadata of this directory
    pub meta: RefCell<Meta>,

    /// The contained data of this `Dir`
    pub(crate) data: HashMap<String, Entry>,
}

impl Dir {
    pub fn write_data<W: Write, R: Read + Seek>(
        &self,
        off: &mut u64,
        writer: &mut W,
        reader: &mut R,
        prog: &ProgressBar,
    ) -> std::io::Result<Entry> {
        Ok(Entry::Dir(Self {
            meta: self.meta.clone(),
            data: self
                .data
                .iter()
                .map(
                    |(key, val)| match val.write_file_data(off, writer, reader, prog) {
                        Ok(val) => Ok((key.clone(), val)),
                        Err(e) => Err(e),
                    },
                )
                .collect::<Result<HashMap<String, Entry>, _>>()?,
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
    pub fn entry(&self, paths: impl AsRef<path::Path>) -> Option<&Entry> {
        self.get_entry(paths.as_ref().components())
    }
    #[inline]
    pub fn entry_mut(&mut self, paths: impl AsRef<path::Path>) -> Option<&mut Entry> {
        self.get_entry_mut(paths.as_ref().components())
    }

    /// Get an iterator over the contained entries
    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.data.iter().map(|(_, entry)| entry)
    }

    /// Get a mutable iterator over the contained entries
    #[inline]
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.data.iter_mut().map(|(_, entry)| entry)
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
    pub fn entry(&self, paths: impl AsRef<path::Path>) -> Option<&Entry> {
        self.get_entry(paths.as_ref().components())
    }

    /// Get a mutable reference to an `Entry`.
    /// For more information, see [get_entry](Entry::get_entry)
    #[inline]
    pub fn entry_mut(&mut self, paths: impl AsRef<path::Path>) -> Option<&mut Entry> {
        self.get_entry_mut(paths.as_ref().components())
    }

    /// Write file data to a writer, returning new headers with updated offsets
    pub(crate) fn write_file_data<W: Write, R: Read + Seek>(
        &self,
        off: &mut u64,
        writer: &mut W,
        reader: &mut R,
        prog: &ProgressBar,
    ) -> std::io::Result<Entry> {
        match self {
            Self::Dir(dir) => dir.write_data(off, writer, reader, prog),
            Self::File(file) => file.write_data(off, writer, reader, prog),
        }
    }

    pub const fn as_dir(&self) -> Option<&Dir> {
        match self {
            Self::Dir(dir) => Some(dir),
            _ => None,
        }
    }

    pub fn as_dir_mut(&mut self) -> Option<&mut Dir> {
        match self {
            Self::Dir(dir) => Some(dir),
            _ => None,
        }
    }

    pub const fn as_file(&self) -> Option<&File> {
        match self {
            Self::File(file) => Some(file),
            _ => None,
        }
    }

    pub fn as_file_mut(&mut self) -> Option<&mut File> {
        match self {
            Self::File(file) => Some(file),
            _ => None,
        }
    }

    /// Get the name of this file or directory
    #[inline(always)]
    pub fn name(&self) -> String {
        match self {
            Self::Dir(dir) => dir.meta.borrow().name.clone(),
            Self::File(file) => file.meta.borrow().name.clone(),
        }
    }

    /// Get the metadata of this entry
    pub fn meta(&self) -> std::cell::Ref<Meta> {
        match self {
            Self::Dir(ref dir) => dir.meta.borrow(),
            Self::File(ref file) => file.meta.borrow(),
        }
    }

    /// Get a mutable reference to this entry's metadata
    pub fn meta_mut(&self) -> std::cell::RefMut<Meta> {
        match self {
            Self::File(f) => f.meta.borrow_mut(),
            Self::Dir(d) => d.meta.borrow_mut(),
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
                meta: RefCell::new(Meta {
                    name: "test".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        let mut root = Entry::Dir(root);
        match root.entry_mut("test").unwrap() {
            Entry::Dir(dir) => dir.data.insert(
                "test.txt".into(),
                Entry::File(File {
                    meta: RefCell::new(Meta {
                        name: "test.txt".into(),
                        ..Default::default()
                    }),
                    compression: "none".parse().unwrap(),
                    off: 0,
                    size: 0,
                    enc: EncryptType::None,
                }),
            ),
            _ => panic!("Not a directory!"),
        };
        let _ = root.entry("test/test.txt").unwrap();
    }
}
