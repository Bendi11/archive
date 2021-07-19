pub mod bar;
pub mod entry;

use bar::{ser_header, Header};
pub use bar::{Bar, BarErr, BarResult};
use byteorder::{LittleEndian, WriteBytesExt};
use indicatif::{ProgressBar, ProgressStyle};

use entry::{CompressType, Entry, Meta};
use std::cell::RefCell;
use std::io::{self, Seek, SeekFrom, Write};

impl<S: io::Read + io::Write + io::Seek> Bar<S> {
    /// Pack an entire directory into a `Bar` struct using a given compression method for every file
    /// This function takes an absolute or relative path to a directory that will be packed, the directory
    /// name will be used as the archive's name
    pub fn pack(
        dir: impl AsRef<std::path::Path>,
        mut backend: S,
        compression: CompressType,
        prog: bool,
    ) -> BarResult<Self> {
        let prog = match prog {
            true => ProgressBar::new_spinner()
                .with_style(ProgressStyle::default_spinner().tick_chars(".,'`*@*`',")),
            false => ProgressBar::hidden(),
        };
        let dir = dir.as_ref();
        let mut off = 0u64; //The current offset into the backing storage

        let meta = Self::read_all_entry_metadata(dir.join(Self::ROOT_METADATA_FILE))?;
        let root_meta = if let Some(meta) = meta.get("/") {
            meta.clone()
        } else {
            Meta {
                name: dir.file_name().unwrap().to_str().unwrap().to_owned(),
                ..Default::default()
            }
        };

        Ok(Self {
            header: Header {
                meta: root_meta,
                root: entry::Dir {
                    meta: RefCell::new(Meta {
                        name: "root".to_owned(),
                        ..Default::default()
                    }),
                    data: Self::pack_read_dir(
                        dir,
                        &mut off,
                        &mut backend,
                        &meta,
                        compression,
                        &prog,
                    )?
                    .into_iter()
                    .map(|entry| (entry.name(), entry))
                    .collect(),
                },
            },
            data: backend,
        })
    }
}

impl<S: io::Read + io::Seek> Bar<S> {
    /// Get the metadata of this bar archive
    pub fn meta(&self) -> &Meta {
        &self.header.meta
    }

    /// Unpack a packed archive from a file or other storage, like an in-memory byte array.
    /// See also [unpack](fn@Bar::unpack)
    pub fn unpack_reader(mut storage: S) -> BarResult<Self> {
        let header = Self::read_header(&mut storage)?;

        Ok(Self {
            header,
            data: storage,
        })
    }

    /// Get a reference to an entry in the Bar archive. This should
    /// NOT contain a root symbol like '/' on linux or
    /// 'C:\\' on windows
    #[inline]
    pub fn entry(&self, path: impl AsRef<std::path::Path>) -> Option<&Entry> {
        self.header.root.entry(path)
    }

    /// See [entry](fn@Bar::entry)
    #[inline]
    pub fn entry_mut(&mut self, path: impl AsRef<std::path::Path>) -> Option<&mut Entry> {
        self.header.root.entry_mut(path)
    }

    /// Get a mutable reference to the root directory
    #[inline]
    pub fn root_mut(&mut self) -> &mut entry::Dir {
        &mut self.header.root
    }

    /// Get an entry and ensure that is a [File](entry::File), returning `None` if either
    /// the entry does not exist or if the entry is not a file
    #[inline]
    pub fn file_mut(&mut self, path: impl AsRef<std::path::Path>) -> Option<&mut entry::File> {
        self.header
            .root
            .entry_mut(path)
            .map(|e| e.as_file_mut())
            .flatten()
    }

    #[inline]
    pub fn dir_mut(&mut self, path: impl AsRef<std::path::Path>) -> Option<&mut entry::Dir> {
        self.header
            .root
            .entry_mut(path)
            .map(|e| e.as_dir_mut())
            .flatten()
    }

    #[inline]
    pub fn dir(&self, path: impl AsRef<std::path::Path>) -> Option<&entry::Dir> {
        self.header.root.entry(path).map(|e| e.as_dir()).flatten()
    }

    /// Save this archive to a directory, decompressing all contained files
    pub fn save_unpacked(
        &mut self,
        path: impl AsRef<std::path::Path>,
        prog: bool,
    ) -> BarResult<()> {
        let path = path.as_ref();
        let dir = path.join(self.header.meta.name.clone());
        std::fs::create_dir_all(dir.clone())?; //Create the dir to save unpacked files to

        let metafile = dir.join(Self::ROOT_METADATA_FILE);
        let metadata = self.all_entry_metadata(&dir);
        let mut metafile = std::fs::File::create(metafile)?;
        rmpv::encode::write_value(&mut metafile, &metadata)?;

        for (_, entry) in self.header.root.data.iter() {
            Self::save_entry(dir.as_ref(), entry, &mut self.data, prog, true, true)?;
        }

        Ok(())
    }

    /// Get a reference to a file contained in this archive if the file exists
    #[inline]
    pub fn file(&self, path: impl AsRef<std::path::Path>) -> Option<&entry::File> {
        self.header.root.entry(path).map(|e| e.as_file()).flatten()
    }

    /// Save this archive to any type implementing `Write`, compressing files as needed
    pub fn save<W: io::Write>(&mut self, writer: &mut W, prog: bool) -> BarResult<()> {
        let prog = match prog {
            true => ProgressBar::new_spinner()
                .with_style(ProgressStyle::default_spinner().tick_chars(".,'`*`',")),
            false => ProgressBar::hidden(),
        };
        prog.enable_steady_tick(33);

        self.data.seek(SeekFrom::Start(0))?;
        let mut data_size = 0u64;
        let root =
            match self
                .header
                .root
                .write_data(&mut data_size, writer, &mut self.data, &prog)?
            {
                Entry::Dir(dir) => dir,
                _ => unreachable!(),
            };
        self.header.root = root;
        let header = ser_header(&self.header);
        rmpv::encode::write_value(writer, &header)?; //Write the header to the output
        writer.write_u64::<LittleEndian>(data_size)?; //Write the file data size to the output

        writer.flush()?;
        Ok(())
    }

    /// Return the root folder of the archive that contains all subfolders and files
    #[inline]
    #[must_use]
    pub fn root(&self) -> &entry::Dir {
        &self.header.root
    }
    /// Return an iterator over all entries in this archive
    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.header.root.entries()
    }

    /// Return a mutable iterator over all entries in the archive
    #[inline]
    pub fn entries_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut Entry> {
        self.header.root.entries_mut()
    }

    /// Write file data to a writer if the file exists, optionally decompressing the file's data
    pub fn file_data(
        &mut self,
        file: entry::File,
        w: &mut impl io::Write,
        decompress: bool,
        prog: bool,
    ) -> BarResult<()> {
        Self::save_file(&file, w, &mut self.data, decompress, prog)
    }

    /// Save a file entry to a file, or a folder to a real folder, if the recurse parameter is
    /// `true`
    pub fn entry_data(
        &mut self,
        dir: impl AsRef<std::path::Path>,
        entry: entry::Entry,
        decompress: bool,
        prog: bool,
        recurse: bool,
    ) -> BarResult<()> {
        let path = dir.as_ref().join(entry.name());

        match entry {
            Entry::Dir(dir) => {
                let dirprog = match prog {
                    true => ProgressBar::new(dir.data.len() as u64)
                        .with_style(ProgressStyle::default_bar().progress_chars("=>-")),
                    false => ProgressBar::hidden(),
                };

                dirprog.set_message(format!("Saving directory {}", dir.meta.borrow().name));
                std::fs::create_dir_all(path.clone())?;
                for (_, file) in dir.data.iter() {
                    Self::save_entry(
                        path.as_ref(),
                        file,
                        &mut self.data,
                        prog,
                        decompress,
                        recurse,
                    )?;
                    dirprog.inc(1);
                }
                dirprog.finish_and_clear();
            }
            Entry::File(ref file) => {
                let mut file_data = std::fs::File::create(path)?;
                Self::save_file(file, &mut file_data, &mut self.data, decompress, prog)?;
            }
        }
        Ok(())
    }
}

impl Bar<std::fs::File> {
    /// Unpack an archive file into a `Bar` struct, returning `Self` if the archive is valid.
    /// Note that this function performs very little, as it does not read archive file data, only
    /// header entries.
    /// ## Example
    /// ```no_run
    /// # use ::bar::Bar;
    /// # fn main() {
    /// let archive = Bar::unpack("./archive.bar", true).unwrap();
    /// # }
    /// ```
    pub fn unpack(file: impl AsRef<std::path::Path>) -> BarResult<Self> {
        let file = file.as_ref();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(file)?;
        Self::unpack_reader(file)
    }

    /// Re-save a bar file with updated metadata
    pub fn save_updated(mut self, prog: bool) -> BarResult<()> {
        let (header_pos, _) = Self::get_header_pos(&mut self.data)?;
        self.data.set_len(header_pos)?; //Truncate the underlying file to erase the file data size and header data
        self.data.seek(io::SeekFrom::End(0))?;
        let val = bar::ser_header(&self.header); //Serialize our header with updated metadata

        let prog = match prog {
            true => ProgressBar::new(0).with_style(
                ProgressStyle::default_bar()
                    .template("[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec} {msg}"),
            ),
            false => ProgressBar::hidden(),
        };

        prog.set_message("Re-writing updated header values to file");
        rmpv::encode::write_value(&mut prog.wrap_write(&mut self.data), &val)?;
        prog.finish_and_clear();
        self.data.write_u64::<LittleEndian>(header_pos)?;
        self.data.flush()?;
        Ok(())
    }
}
