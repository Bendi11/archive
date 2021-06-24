//! The `ar` module provides structs representing a bar archive file that can be deserialized and serilialzed
//!

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use chrono::{DateTime, NaiveDateTime, Utc};
use entry::Entry;
use rmpv::Value;
use std::{collections::HashMap, fmt, io::{self, Read, Seek, SeekFrom, Write}, str::FromStr};
use thiserror::Error;

use self::entry::{Dir, Meta};

pub mod entry;

/// The `Bar` struct contains methods to read, manipulate and create `bar` files
/// using any type that implements `Seek`, `Read` and `Write`
pub struct Bar<S: Read + Write + Seek> {
    /// The internal data that we read from and write to
    data: S,

    /// The header data
    header: Header,
}

impl<S: Read + Seek + Write> fmt::Debug for Bar<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.header)
    }
}

/// The root header containing top level metadata and the root directory
#[derive(Debug, Clone)]
pub struct Header {
    /// Metadata about the entire archive
    pub meta: Meta,

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
const LASTUPDATE: u8 = 7;
const USED: u8 = 8;
const COMPRESSMETHOD: u8 = 9;

impl Bar<io::Cursor<Vec<u8>>> {
    /// Create a new `Bar` reader / writer from any type implementing `Seek`, `Read` and `Write`
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
                root: entry::Dir {
                    meta: Meta {
                        name: "root".to_owned(),
                        ..Default::default()
                    },
                    data: HashMap::new()
                }
            }
        }
    }
}

impl Bar<std::fs::File> {
    pub fn unpack(file: impl AsRef<std::path::Path>) -> BarResult<Self> {
        let file = file.as_ref();
        let file = std::fs::File::open(file)?;
        Self::unpack_reader(file)
    }
}

impl<S: Read + Write + Seek> Bar<S> {
    const METADATAFILENAME: &'static str = "-__META__.internaldata.msgpack";

    fn pack_read_dir<W: Write>(dir: &std::path::Path, off: &mut u64, writer: &mut W) -> BarResult<Vec<Entry>> {
        let mut vec = vec![];
        let mut meta_vec = HashMap::new(); //A map of file or directory names to metadata structs

        for file in std::fs::read_dir(dir)? {
            let file = file?;
            let name = file.file_name().to_str().unwrap().to_owned();

            if name.ends_with(Self::METADATAFILENAME) {
                let mut file = std::fs::File::open(file.path())?; //Open the metadata file 
                let meta = rmpv::decode::read_value(&mut file)?; //Read the header value
                let meta = Self::read_meta(&meta)?; 
                meta_vec.insert(name, meta);
                continue
            }
        }

        for file in std::fs::read_dir(dir)? {
            let file = file?;
            let name = file.file_name().to_str().unwrap().to_owned();

            if name.ends_with(Self::METADATAFILENAME) {
                continue
            }

            let meta = match meta_vec.get(&name) {
                Some(meta) => meta.clone(),
                None => Meta {
                    name: name.clone(),
                    ..Default::default()
                }
            };

            match file.metadata().unwrap().is_dir() {
                true => {
                    let directory = entry::Dir {
                        meta,
                        data: Self::pack_read_dir(&file.path(), off, writer)?.into_iter().map(|entry| (entry.name(), entry)).collect()
                    };
                    vec.push(Entry::Dir(directory));
                    
                },
                false => {
                    let mut data = std::fs::File::open(file.path())?; //Open the file at the given location
                    let size = data.metadata()?.len();
                    
                    let file = entry::File {
                        compression: entry::CompressMethod::None,
                        off: *off,
                        size: size as u32,
                        meta,
                    };
                    *off += size;
                    std::io::copy(&mut data, writer)?;
                    vec.push(Entry::File(file))
                }
            }
        }
        Ok(vec)
    }

    /// Pack a directory into an archive struct using a backing storage for file data
    pub fn pack(dir: impl AsRef<std::path::Path>, mut backend: S) -> BarResult<Self> {
        let dir = dir.as_ref();
        let mut off = 0u64; //The current offset into the backing storage

        let metafilenamne = dir.join(Self::METADATAFILENAME);
        let meta = if metafilenamne.exists() {
            let mut file = std::fs::File::open(metafilenamne)?; //Open the metadata file 
            let meta = rmpv::decode::read_value(&mut file)?; //Read the header value
            Self::read_meta(&meta)?
        } else {
            Meta {
                name: dir.file_name().unwrap().to_str().unwrap().to_owned(),
                ..Default::default()
            }
        };

        Ok(Self { 
            header: Header {
                meta,
                root: entry::Dir {
                    meta: Meta {
                        name: "root".to_owned(),
                        ..Default::default()
                    },
                    data: Self::pack_read_dir(dir, &mut off, &mut backend)?.into_iter().map(|entry| (entry.name(), entry)).collect()
                }
            },
            data: backend,
        })
    }

    /// Write this archive to a type implementing `Write`
    pub fn write<W: Write>(&mut self, writer: &mut W) -> BarResult<()> {
        self.data.seek(SeekFrom::Start(0))?;
        let mut data_size = 0u64; 
        let root = match self.header.root.write_data(&mut data_size, writer, &mut self.data)? {
            Entry::Dir(dir) => dir,
            _ => unreachable!()
        };
        self.header.root = root;
        let header = Self::write_header(&self.header); 
        rmpv::encode::write_value(writer, &header)?; //Write the header to the output 
        println!("Wrote {} to file", data_size);
        writer.write_u64::<LittleEndian>(data_size)?; //Write the file data size to the output

        writer.flush()?;
        Ok(())
    }

    /// Create a file entry from a `File` entry
    fn write_file_entry(file: &entry::File) -> Value {
        use rmpv::{Integer, Utf8String};
        Value::Map(vec![
            (Value::Integer(Integer::from(OFFSET)), Value::Integer(Integer::from(file.off))),
            (Value::Integer(Integer::from(SIZE)), Value::Integer(Integer::from(file.size))),
            (Value::Integer(Integer::from(META)), Self::write_meta(&file.meta)),
            (Value::Integer(Integer::from(COMPRESSMETHOD)), Value::String(Utf8String::from(file.compression.to_string()))),
        ])
    }

    fn write_meta(meta: &Meta) -> Value {
        use rmpv::{Integer, Utf8String};
        let mut map = vec![
            (Value::Integer(Integer::from(NAME)), Value::String(Utf8String::from(meta.name.clone()))),
            (Value::Integer(Integer::from(USED)), Value::Boolean(meta.used)),
        ];
        if meta.last_update.is_some() {
            map.push((Value::Integer(Integer::from(LASTUPDATE)), Value::Integer(Integer::from(meta.last_update.unwrap().timestamp()))))
        }
        if meta.note.is_some() {
            map.push((Value::Integer(Integer::from(COMPRESSMETHOD)), Value::String(Utf8String::from(meta.note.clone().unwrap()))))
        }
        
        Value::Map(map)
    }

    fn write_entry(entry: &Entry) -> Value {
        match entry {
            Entry::Dir(dir) => Value::Array(vec![
                Value::Boolean(false),
                Self::write_dir_entry(dir)
            ]),
            Entry::File(file) => Value::Array(vec![
                Value::Boolean(true),
                Self::write_file_entry(file)
            ])
        }
    }

    fn write_dir_entry(dir: &entry::Dir) -> Value {
        Value::Array(vec![
            Self::write_meta(&dir.meta),
            Value::Array(dir.data.iter().map(|(_, val)| Self::write_entry(val)).collect::<Vec<Value>>())
        ])
    }

    fn write_header(header: &Header) -> Value {
        Value::Array(vec![
            Self::write_meta(&header.meta),
            Self::write_dir_entry(&header.root),
        ])
    }


    /// Unpack an archive file into a [Bar] struct
    pub fn unpack_reader(mut storage: S) -> BarResult<Self> {
        let header = Self::read_header(&mut storage)?;
        Ok(Self {
            header,
            data: storage
        })
    }

    /// Read a file entry from the header
    fn read_file_entry(val: &Value) -> BarResult<entry::File> {
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
        let compression = entry::CompressMethod::from_str(compression).map_err(|e| {
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
            meta,
            compression,
        })
    }

    /// Read a directory entry from a header value
    fn read_dir_entry(val: &Value) -> BarResult<entry::Dir> {
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
                    meta,
                })
            }
            _ => Err(BarErr::InvalidHeaderFormat(format!(
                "Directory entry array is not 2 entries large, it is {} long",
                val.len()
            ))),
        }
    }

    /// Read a metada map from a value
    fn read_meta(val: &Value) -> BarResult<Meta> {
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
                    last_update: map.get(&(LASTUPDATE as u64)).map_or(
                        Result::<_, BarErr>::Ok(None),
                        |val| {
                            Ok(Some(DateTime::<Utc>::from_utc(
                                NaiveDateTime::from_timestamp(
                                    val.as_i64().ok_or_else(|| {
                                        BarErr::InvalidHeaderFormat(
                                            "Last update field of metadata is not a u64".into(),
                                        )
                                    })?,
                                    0,
                                ),
                                Utc,
                            )))
                        },
                    )?,
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

    /// Read header bytes from the internal reader by seeking to the end and reading the file size
    fn read_header(data: &mut S) -> BarResult<Header> {
        data.seek(SeekFrom::End(0))?; //Seek to the end of the file, then back 4 bytes
        let file_size = data.stream_position()?;
        data.seek(SeekFrom::End(-8))?;

        let data_size = data.read_u64::<LittleEndian>()?;
        println!("Seek to {}, file size {}, pos: {}", data_size, file_size, data.stream_position()?);
        let header_size = (file_size - data_size) - 8;
        data.seek(SeekFrom::Start(data_size))?;
        
        let mut header_bytes = vec![0u8; header_size as usize];
        data.read_exact(&mut header_bytes)?;

        let header_val = rmpv::decode::read_value(&mut header_bytes.as_slice())?; //Read the value from the header bytes
        let header_val = header_val.as_array().ok_or(BarErr::InvalidHeaderFormat(
            format!("The top level header is not an array, it is a {:?}", header_val),
        ))?;
        match (header_val.get(0), header_val.get(1)) {
            (Some(metadata), Some(root)) => {
                let meta = Self::read_meta(metadata)?; //Get the metadata of the header
                let dir = Self::read_dir_entry(root)?;
                Ok(Header { meta, root: dir })
            }
            _ => {
                return Err(BarErr::InvalidHeaderFormat(
                    "The top level header array does not contain two elements".into(),
                ))
            }
        }
    }

    /// Read an entry from the file, assumes that the reader is positioned just before a header nibble
    /// Entry: Array [
    /// Boolean (DIR is false, FILE is true),
    /// if DIR <Directory>
    /// if FILE <File>   
    /// ]
    fn read_entry(val: &Value) -> BarResult<Entry> {
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


    /// Save this archive to a directory without packing
    pub fn save_unpacked(&mut self, path: impl AsRef<std::path::Path>) -> BarResult<()> {
        let path = path.as_ref();
        let dir = path.join(self.header.meta.name.clone());
        std::fs::create_dir_all(dir.clone())?; //Create the dir to save unpacked files to

        let metafile = dir.join(self.header.meta.name.clone());
        Self::save_meta_to_file(metafile.as_ref(), &self.header.meta)?; //Save header metadata to a file
        for (_, entry) in self.header.root.data.iter() {
            Self::save_entry(dir.as_ref(), entry, &mut self.data)?;
        }

        Ok(())
    }

    /// Save an entry to a file or to a folder if it is a [Dir](Entry::Dir)
    fn save_entry(dir: &std::path::Path, entry: &Entry, back: &mut S) -> BarResult<()> {
        let path = dir.join(entry.name());
        Self::save_meta_to_file(&path, entry.meta())?;

        match entry {
            Entry::Dir(dir) => {
                std::fs::create_dir_all(path.clone())?;
                for (_, file) in dir.data.iter() {
                    Self::save_entry(path.as_ref(), file, back)?;
                }
            },
            Entry::File(file) => {
                let mut file_data = std::fs::File::create(path)?;
                let mut data = vec![0u8 ; file.size as usize];
                back.seek(SeekFrom::Start(file.off))?;
                back.read_exact(&mut data)?;
                io::copy(&mut data.as_slice(), &mut file_data)?;
                drop(file_data);
            }
        }
        Ok(())
    }

    fn save_meta_to_file(filename: &std::path::Path, meta: &Meta) -> BarResult<()> {
        let path = filename.with_file_name(filename.file_name().unwrap().to_str().unwrap().to_owned() + Self::METADATAFILENAME);
        let mut file = std::fs::File::create(path)?;
        let val = Self::write_meta(meta);
        rmpv::encode::write_value(&mut file, &val)?;
        drop(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    pub fn test_write() {
        //let mut thing = Bar::new("test_archive");
        let back = io::Cursor::new(vec![0u8 ; 2048]);
        let mut thing = Bar::pack("test", back).unwrap();
        let mut file = io::BufWriter::new(std::fs::File::create("./archive.bar").unwrap());
        thing.write(&mut file).unwrap();
        drop(thing);
        let mut reader = Bar::unpack("./archive.bar").unwrap();
        reader.save_unpacked("output").unwrap();
    }
}