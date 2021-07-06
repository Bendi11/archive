//! The `ar` module provides structs representing a bar archive file that can be deserialized and serilialzed
//!

use super::entry;
use chacha20poly1305::Nonce;
use super::entry::Entry;
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::{DeflateDecoder, GzDecoder};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use rmpv::Value;
use std::cell::Cell;
use std::convert;
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    io::{self, Read, Seek, SeekFrom, Write},
    path,
    str::FromStr,
};
use thiserror::Error;

use crate::ar::entry::{CompressMethod, CompressType, Dir, Meta};

/// The `Bar` struct contains methods to read, manipulate and create `bar` files
/// using any type that implements `Seek`, `Read` and `Write`
pub struct Bar<S: Read + Seek> {
    /// The internal data that we read from and write to
    pub(super) data: S,

    /// The header data
    pub(super) header: Header,
}

impl<S: Read + Seek> fmt::Debug for Bar<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.header)
    }
}

/// The root header containing top level metadata and the root directory
#[derive(Debug, Clone)]
pub struct Header {
    /// Metadata about the entire archive
    pub meta: Meta,

    /// The nonce counter
    pub nonce: Nonce,

    /// The root directory of the header
    pub root: Dir,
}

/// The `BarErr` enum enumerates all possible errors that can occur when reading from or writing to a
/// bar file
#[derive(Debug, Error)]
pub enum BarErr {
    /// An internal I/O error occurred
    #[error("An internal Input / Output error disrupted bar file reading: {0}")]
    Io(#[from] io::Error),

    /// An unaccepted header nibble was encountered
    #[error("An invalid header value was encountered when decoding bar file header: {0}")]
    InvalidMsgPackDecode(#[from] rmpv::decode::Error),

    #[error("An error occurred when encoding header of bar file: {0}")]
    InvalidMsgPackEncode(#[from] rmpv::encode::Error),

    #[error("The archive header format is invalid: {0}")]
    InvalidHeaderFormat(String),

    #[error("The metadata file format is invalid: {0}")]
    BadMetadataFile(String),

    #[error("An error occurred while encrypting/decrypting a file {0}")]
    EncryptError(chacha20poly1305::aead::Error),

    #[error("The specified entry at path {0} does not exist")]
    NoEntry(String),
}

impl convert::From<chacha20poly1305::aead::Error> for BarErr {
    fn from(e: chacha20poly1305::aead::Error) -> Self {
        Self::EncryptError(e)
    }
}

/// The `BarResult<T>` type is a result with an Err variant of [BarErr]
pub type BarResult<T> = Result<T, BarErr>;

const NOTE: u8 = 0;
const NAME: u8 = 1;
const META: u8 = 2;
const _FILE: u8 = 3;
const _DIR: u8 = 4;
const OFFSET: u8 = 5;
const SIZE: u8 = 6;
const ENCRYPTION: u8 = 7;
const USED: u8 = 8;
const COMPRESSMETHOD: u8 = 9;

pub(super) fn ser_meta(meta: &Meta) -> Value {
    use rmpv::{Integer, Utf8String};
    let mut map = vec![
        (
            Value::Integer(Integer::from(NAME)),
            Value::String(Utf8String::from(meta.name.clone())),
        ),
        (
            Value::Integer(Integer::from(USED)),
            Value::Boolean(meta.used),
        ),
    ];
    if meta.note.is_some() {
        map.push((
            Value::Integer(Integer::from(NOTE)),
            Value::String(Utf8String::from(meta.note.clone().unwrap())),
        ))
    }

    Value::Map(map)
}

pub(super) fn ser_entry(entry: &Entry) -> Value {
    match entry {
        Entry::Dir(dir) => Value::Array(vec![Value::Boolean(false), ser_direntry(dir)]),
        Entry::File(file) => Value::Array(vec![Value::Boolean(true), ser_fileentry(file)]),
    }
}

pub(super) fn ser_direntry(dir: &entry::Dir) -> Value {
    Value::Array(vec![
        ser_meta(&dir.meta.borrow()),
        Value::Array(
            dir.data
                .iter()
                .map(|(_, val)| ser_entry(val))
                .collect::<Vec<Value>>(),
        ),
    ])
}

pub(super) fn ser_header(header: &Header) -> Value {
    Value::Array(vec![ser_meta(&header.meta), ser_direntry(&header.root)])
}

/// Create a file value from a `File` entry
pub(super) fn ser_fileentry(file: &entry::File) -> Value {
    use rmpv::{Integer, Utf8String};
    let mut map = vec![
        (
            Value::Integer(Integer::from(OFFSET)),
            Value::Integer(Integer::from(file.off)),
        ),
        (
            Value::Integer(Integer::from(SIZE)),
            Value::Integer(Integer::from(file.size)),
        ),
        (
            Value::Integer(Integer::from(META)),
            ser_meta(&file.meta.borrow()),
        ),
        (
            Value::Integer(Integer::from(COMPRESSMETHOD)),
            Value::String(Utf8String::from(file.compression.to_string())),
        ),

    ];
    if file.is_encrypted() {
        let nonce = match file.enc.get() {
            entry::EncryptType::ChaCha20(nonce) => nonce,
            _ => unreachable!()
        };
        map.push(
            (
                Value::Integer(Integer::from(ENCRYPTION)),
                Value::Binary(nonce.to_vec())
            )
        )
    }

    Value::Map(map)
}

impl Bar<io::Cursor<Vec<u8>>> {
    /// Create a new `Bar` archive with an in-memory `Vec` as backing storage
    #[inline]
    #[must_use]
    pub fn new(name: impl ToString) -> Self {
        Self {
            data: io::Cursor::new(Vec::new()),
            header: Header {
                meta: Meta {
                    name: name.to_string(),
                    ..Default::default()
                },
                nonce: Nonce::clone_from_slice(&[0u8 ; 12]),
                root: entry::Dir {
                    meta: RefCell::new(Meta {
                        name: "root".to_owned(),
                        ..Default::default()
                    }),
                    data: HashMap::new(),
                },
            },
        }
    }
}

impl<S: Read + Seek> Bar<S> {
    /// The file name of a metadata file in uncompressed archives
    pub(super) const ROOT_METADATA_FILE: &'static str = ".__barmeta.msgpack";

    /// Get a hashmap of file paths in the archive to their metadata bincode
    pub(super) fn all_entry_metadata(&self, path: impl AsRef<path::Path>) -> Value {
        use rmpv::Utf8String;
        let mut vec = vec![];
        fn walk_dir(vec: &mut Vec<(Value, Value)>, dir: &entry::Dir, path: path::PathBuf) {
            for (name, item) in dir.data.iter() {
                match item {
                    Entry::Dir(dir) => {
                        vec.push((
                            Value::String(Utf8String::from(path.join(name).to_str().unwrap())),
                            ser_meta(&dir.meta.borrow()),
                        ));
                        walk_dir(vec, dir, path.join(name));
                    }
                    Entry::File(file) => vec.push((
                        Value::String(Utf8String::from(path.join(name).to_str().unwrap())),
                        ser_meta(&file.meta.borrow()),
                    )),
                }
            }
        }
        walk_dir(&mut vec, &self.header.root, path.as_ref().to_owned());
        vec.push((
            Value::String(Utf8String::from("/")),
            ser_meta(&self.header.meta),
        ));
        Value::Map(vec)
    }

    /// Read all entry metadata from a root file when packing a previously unpacked directory
    pub(super) fn read_all_entry_metadata(
        file: impl AsRef<std::path::Path>,
    ) -> BarResult<HashMap<String, Meta>> {
        let mut data = match std::fs::File::open(file.as_ref()) {
            Ok(data) => data,
            Err(_) => {
                return Ok(HashMap::new());
            }
        };
        let val = rmpv::decode::read_value(&mut data)?;
        let val = val.as_map().ok_or_else(|| {
            BarErr::BadMetadataFile("Entry metadata file's main content is not a map".into())
        })?;
        val.iter()
            .map(|(path, meta)| -> BarResult<_> {
                let path = path.as_str().ok_or_else(|| {
                    BarErr::BadMetadataFile("The keys for metada's map are not strings".into())
                })?;
                let meta = Self::read_meta(meta)?; //Read the metadata
                Ok((path.to_owned().replace("\\", "/"), meta))
            })
            .collect::<BarResult<HashMap<String, Meta>>>()
    }

    /// Read all files in a directory into a list of [Entry]s, reading metadata files if possible
    pub(super) fn pack_read_dir<W: Write>(
        dir: &std::path::Path,
        off: &mut u64,
        writer: &mut W,
        meta_vec: &HashMap<String, Meta>,
        compress: CompressType,
        prog: &ProgressBar,
    ) -> BarResult<Vec<Entry>> {
        let mut vec = vec![];

        for file in std::fs::read_dir(dir)? {
            let file = file?;
            prog.set_message(format!("Writing file {} to archive", file.path().display()));

            let name = file.file_name().to_str().unwrap().to_owned();

            if name == Self::ROOT_METADATA_FILE {
                continue;
            }

            //See if we have any metadata files to go with this one
            let meta = match meta_vec.get(&file.path().to_str().unwrap().replace("\\", "/")) {
                Some(meta) => meta.clone(),
                None => Meta {
                    name: name.clone(),
                    ..Default::default()
                },
            };

            match file.metadata().unwrap().is_dir() {
                true => {
                    let directory = entry::Dir {
                        meta: RefCell::new(meta),
                        data: Self::pack_read_dir(
                            &file.path(),
                            off,
                            writer,
                            meta_vec,
                            compress,
                            prog,
                        )?
                        .into_iter()
                        .map(|entry| (entry.name(), entry))
                        .collect(),
                    };
                    vec.push(Entry::Dir(directory));
                }
                false => {
                    let read_prog = match prog.is_hidden() {
                        true => ProgressBar::hidden(),
                        false => ProgressBar::new(0).with_style(
                            ProgressStyle::default_bar()
                                .template(
                                    "[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec} {msg}",
                                )
                                .progress_chars("=>-"),
                        ),
                    };

                    let mut data = std::fs::File::open(file.path())?; //Open the file at the given location
                    let size = data.metadata()?.len();


                    let file = entry::File {
                        compression: compress,
                        off: *off,
                        size: size as u32,
                        meta: RefCell::new(meta),
                        enc: Cell::new(entry::EncryptType::None),
                    };
                    *off += size;
                    std::io::copy(&mut read_prog.wrap_read(&mut data), writer)?;
                    read_prog.finish_and_clear();
                    vec.push(Entry::File(file))
                }
            }

            prog.tick();
        }
        Ok(vec)
    }

    /// Read a file entry from the header
    pub(super) fn read_file_entry(val: &Value) -> BarResult<entry::File> {
        let val = val.as_map().ok_or_else(|| {
            BarErr::InvalidHeaderFormat(format!("File field is not an map, it is a {}", val))
        })?;
        let val = val
            .iter()
            .map(|(key, val)| match key {
                Value::Integer(num) => Ok((num.as_u64().unwrap(), val.clone())),
                other => Err(BarErr::InvalidHeaderFormat(format!(
                    "Key for metadata field is not an integer value, it is {}",
                    other
                ))),
            })
            .collect::<BarResult<HashMap<u64, Value>>>()?;
        let meta = val.get(&(META as u64)).ok_or_else(|| {
            BarErr::InvalidHeaderFormat("META field not present in FILE entry".into())
        })?;
        let meta = Self::read_meta(meta)?;

        let compression = val
            .get(&(COMPRESSMETHOD as u64))
            .ok_or_else(|| {
                BarErr::InvalidHeaderFormat("COMPRESSMETHOD field not present in FILE entry".into())
            })?
            .as_str()
            .ok_or_else(|| {
                BarErr::InvalidHeaderFormat(
                    "COMPRESSMETHOD field in FILE entry is not a string".into(),
                )
            })?;
        let compression = entry::CompressType::from_str(compression).map_err(|e| {
            BarErr::InvalidHeaderFormat(format!("Unrecognized compression method {}", e))
        })?;
        Ok(entry::File {
            off: val
                .get(&(OFFSET as u64))
                .ok_or_else(|| {
                    BarErr::InvalidHeaderFormat("OFFSET field not present in FILE entry".into())
                })?
                .as_u64()
                .ok_or_else(|| {
                    BarErr::InvalidHeaderFormat("OFFSET field in FILE entry is not a u64".into())
                })?,
            size: val
                .get(&(SIZE as u64))
                .ok_or_else(|| {
                    BarErr::InvalidHeaderFormat("SIZE field not present in FILE entry".into())
                })?
                .as_u64()
                .ok_or_else(|| {
                    BarErr::InvalidHeaderFormat("SIZE field in FILE entry is not a u64".into())
                })? as u32,
            meta: RefCell::new(meta),
            enc: std::cell::Cell::new(match val.get(&(ENCRYPTION as u64)) {
                Some(nonce) => entry::EncryptType::ChaCha20(Nonce::clone_from_slice(nonce.as_slice().ok_or_else(|| {
                    BarErr::InvalidHeaderFormat(
                        "ENC field in FILE entry is present but is not an array".into(),
                    )
                })?)),
                None => entry::EncryptType::None,
            }),
            compression,
        })
    }

    /// Read a directory entry from a header value
    pub(super) fn read_dir_entry(val: &Value) -> BarResult<entry::Dir> {
        let val = val.as_array().ok_or_else(|| {
            BarErr::InvalidHeaderFormat(format!("Directory field is not an array, it is a {}", val))
        })?;
        match (val.get(0), val.get(1)) {
            (Some(meta), Some(files)) => {
                let meta = Self::read_meta(meta)?;
                let files = files.as_array().ok_or_else(|| {
                    BarErr::InvalidHeaderFormat(format!(
                        "Directory files item is not an array, it is a {}",
                        files
                    ))
                })?;
                let files = files
                    .iter()
                    .map(|val| Self::read_entry(val))
                    .collect::<BarResult<Vec<Entry>>>()?;
                Ok(entry::Dir {
                    data: files
                        .into_iter()
                        .map(|entry| (entry.name(), entry))
                        .collect(),
                    meta: RefCell::new(meta),
                })
            }
            _ => Err(BarErr::InvalidHeaderFormat(format!(
                "Directory entry array is not 2 entries large, it is {} long",
                val.len()
            ))),
        }
    }

    /// Read a metada map from a value
    pub(super) fn read_meta(val: &Value) -> BarResult<Meta> {
        match val {
            Value::Map(map) => {
                let map = map
                    .iter()
                    .map(|(key, val)| match key {
                        Value::Integer(num) => Ok((num.as_u64().unwrap(), val.clone())),
                        other => Err(BarErr::InvalidHeaderFormat(format!(
                            "Key for metadata field is not an integer value, it is {}",
                            other
                        ))),
                    })
                    .collect::<BarResult<HashMap<u64, Value>>>()?;
                Ok(Meta {
                    name: map
                        .get(&(NAME as u64))
                        .map_or(Result::<_, BarErr>::Ok(None), |val| {
                            Ok(Some(
                                val.as_str()
                                    .ok_or_else(|| {
                                        BarErr::InvalidHeaderFormat(
                                            "The NAME field of metadata is not a string".into(),
                                        )
                                    })?
                                    .to_owned(),
                            ))
                        })?
                        .ok_or_else(|| {
                            BarErr::InvalidHeaderFormat(
                                "The NAME field of metadata is not present".into(),
                            )
                        })?,
                    used: map
                        .get(&(USED as u64))
                        .unwrap_or(&Value::Boolean(false))
                        .as_bool()
                        .ok_or_else(|| {
                            BarErr::InvalidHeaderFormat(
                                "USED field of metadata is not a boolean".into(),
                            )
                        })?,
                    note: map
                        .get(&(NOTE as u64))
                        .map_or(Result::<_, BarErr>::Ok(None), |val| {
                            Ok(Some(
                                val.as_str()
                                    .ok_or_else(|| {
                                        BarErr::InvalidHeaderFormat(
                                            "The NOTE field of metadata is not a string".into(),
                                        )
                                    })?
                                    .to_owned(),
                            ))
                        })?,
                })
            }
            other => Err(BarErr::InvalidHeaderFormat(format!(
                "Metadata field is not a map, it is a {}",
                other
            ))),
        }
    }

    /// Get the position in the reader that our header data starts and return
    /// (file data size, header size)
    pub(super) fn get_header_pos(data: &mut S) -> BarResult<(u64, u64)> {
        data.seek(SeekFrom::End(0))?; //Seek to the end of the file, then back 8 bytes
        let file_size = data.stream_position()?;
        data.seek(SeekFrom::End(-8))?;

        let data_size = data.read_u64::<LittleEndian>()?;
        let header_size = (file_size - data_size) - 8;
        data.seek(SeekFrom::Start(data_size))?;

        Ok((data_size, header_size))
    }

    /// Read header bytes from the internal reader by seeking to the end and reading the file size
    pub(super) fn read_header(data: &mut S) -> BarResult<Header> {
        let (_, header_size) = Self::get_header_pos(data)?;
        let mut header_bytes = vec![0u8; header_size as usize];
        data.read_exact(&mut header_bytes)?;

        let header_val = rmpv::decode::read_value(&mut header_bytes.as_slice())?; //Read the value from the header bytes
        let header_val = header_val.as_array().ok_or_else(|| {
            BarErr::InvalidHeaderFormat(format!(
                "The top level header is not an array, it is a {:?}",
                header_val
            ))
        })?;
        match (header_val.get(0), header_val.get(1), header_val.get(2)) {
            (Some(metadata), Some(nonce), Some(root)) => {
                let meta = Self::read_meta(metadata)?; //Get the metadata of the header
                let dir = Self::read_dir_entry(root)?;
                let nonce = nonce.as_slice().ok_or_else(|| BarErr::InvalidHeaderFormat("The nonce of the header is not a byte slice".into()))?;
                Ok(Header { meta, root: dir, nonce: Nonce::clone_from_slice(nonce) })
            }
            _ => Err(BarErr::InvalidHeaderFormat(
                "The top level header array does not contain two elements".into(),
            )),
        }
    }

    /// Entry: Array [
    /// Boolean (DIR is false, FILE is true),
    /// if DIR <Directory>
    /// if FILE <File>   
    /// ]
    pub(super) fn read_entry(val: &Value) -> BarResult<Entry> {
        let val = val
            .as_array()
            .ok_or_else(|| BarErr::InvalidHeaderFormat("An entry field is not an array".into()))?;
        match (val.get(0), val.get(1)) {
            (Some(is_dir), Some(entry)) => {
                let is_file = is_dir.as_bool().ok_or_else(|| {
                    BarErr::InvalidHeaderFormat("Entry flag is not a boolean".into())
                })?;
                match is_file {
                    true => Ok(Entry::File(Self::read_file_entry(entry)?)),
                    false => Ok(Entry::Dir(Self::read_dir_entry(entry)?)),
                }
            }
            _ => Err(BarErr::InvalidHeaderFormat(format!(
                "Entry array is not long enough, need len of 2 but len is {}",
                val.len()
            ))),
        }
    }

    /// Save a file's contents to a Writer, optionally decompressing the file's data
    pub(super) fn save_file(
        file: &entry::File,
        writer: &mut impl Write,
        back: &mut S,
        decompress: bool,
        prog: bool,
    ) -> BarResult<()> {
        let prog = match prog {
            true => ProgressBar::new(file.size as u64).with_style(
                ProgressStyle::default_bar()
                    .template("[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec} {msg}")
                    .progress_chars("=>-"),
            ),
            false => ProgressBar::hidden(),
        };

        let mut data = vec![0u8; file.size as usize];
        back.seek(SeekFrom::Start(file.off))?;
        prog.wrap_read(back).read_exact(&mut data)?;
        prog.reset();

        prog.set_message(format!("Saving file {}", file.meta.borrow().name));

        let bytes = match decompress {
            true => match file.compression {
                CompressType(_, CompressMethod::Deflate) => {
                    let mut encoder = DeflateDecoder::new(data.as_slice());

                    let mut decoded = Vec::with_capacity(file.size as usize);
                    encoder.read_to_end(&mut decoded)?;
                    drop(data);
                    decoded
                }
                CompressType(_, CompressMethod::Gzip) => {
                    let mut encoder = GzDecoder::new(data.as_slice());
                    let mut decoded = Vec::with_capacity(file.size as usize);
                    encoder.read_to_end(&mut decoded)?;
                    drop(data);
                    decoded
                }
                CompressType(_, CompressMethod::None) => data,
            },
            false => data,
        };
        io::copy(&mut bytes.as_slice(), &mut prog.wrap_write(writer))?;
        prog.finish_and_clear();

        Ok(())
    }

    /// Save an entry to a file or to a folder if it is a [Dir](Entry::Dir), used to save an unpacked directory
    pub(super) fn save_entry(
        dir: &std::path::Path,
        entry: &Entry,
        back: &mut S,
        prog: bool,
        decompress: bool,
        recurse: bool,
    ) -> BarResult<()> {
        let path = dir.join(entry.name());

        match entry {
            Entry::Dir(dir) => {
                let dirprog = match prog {
                    true => ProgressBar::new(dir.data.len() as u64)
                        .with_style(ProgressStyle::default_bar().progress_chars("=>-")),
                    false => ProgressBar::hidden(),
                };

                if recurse {
                    dirprog.set_message(format!("Saving directory {}", dir.meta.borrow().name));
                    std::fs::create_dir_all(path.clone())?;
                    for (_, file) in dir.data.iter() {
                        Self::save_entry(path.as_ref(), file, back, prog, decompress, recurse)?;
                        dirprog.inc(1);
                    }
                }
                dirprog.finish_and_clear();
            }
            Entry::File(file) => {
                let mut file_data = std::fs::File::create(path)?;
                Self::save_file(file, &mut file_data, back, decompress, prog)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    pub fn test_write() {
        let back = io::Cursor::new(vec![0u8; 2048]);
        let mut thing = Bar::pack("test", back, "high-gzip".parse().unwrap(), false).unwrap();
        let mut file = io::BufWriter::new(std::fs::File::create("./archive.bar").unwrap());
        thing.save(&mut file, false).unwrap();
        drop(thing);
        drop(file);
        let mut reader = Bar::unpack("./archive.bar").unwrap();
        let file = reader.file_mut("subdir/test.txt").unwrap();
        file.meta.borrow_mut().note =
            Some("This is a testing note about the file test.txt testing".into());
        drop(file);

        reader.save_unpacked("output", false).unwrap();
        drop(reader);

        let back = io::Cursor::new(vec![0u8; 2048]);
        let _packer = Bar::pack("output/test", back, "high-gzip".parse().unwrap(), false).unwrap();
    }
}
