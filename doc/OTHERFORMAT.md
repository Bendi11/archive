# Archive Format
The archive format is kept relatively simple for ease of use and to hopefully increase speed. It features raw
file data with a central MessagePack-encoded header that maps file data to offsets and sizes

---

```
[ file data (variable size) ] [ header (mskpack, variable size) ] [ file data size (u64) ]
```

---

Header offsets and sizes can be easily calculated: 
- Header offset: 0 + file data size
- Header size: File size - file data size - 8 (for file data size u64)

### Header Format:
The header is encoded in rmp, its format is described here:
Some constants used instead of strings to save space in maps: 
- NOTE: 0,
- NAME: 1,
- META: 2,
- FILE: 3,
- DIR: 4,
- OFFSET: 5,
- SIZE: 6,
- LASTUPDATE: 7,
- USED: 8,
- COMPRESSMETHOD: 9,

```
Header: Array (root) [
    <Meta>,
    <Directory> (root dir) 
]

Directory: Array [
    <Meta>,
    Array (files): <Entry>* Array of files 
]

Entry: Map [
    Boolean (FILE is true, DIR is false),
    <Directory> or <File>
]

File: Map [
    Integer OFFSET: Integer (offset),
    Integer SIZE: Integer (size),
    Integer META: <Meta>
    Integer COMPRESSMETHOD: String(compression method),
]

Meta: Map {
    Integer LASTUPDATE: Integer (last update),
    Integer USED: Boolean (if the file has been used),
    Integer NOTE: String (note),
    Integer NAME: String (name),
}
```
```jsonc
{
    // Metadata field
    "meta": { 
        // The note field can be null or can be a string, but must not be a different type
        "note": null, 
        /// The name field must be present and must be a string
        "name": "name",

    },
    // The root directory, must be present and be named "/"
    "/": {
        // The data field of a directory is a map of files or directories 
        "data": {
            // A file entry has a name and we know that it isn't a directory becuase it doesn't have the data field
            "file1.txt": {
                // In msgpack a u64, it must be present
                "offset": 0,
                // In msgpack, a u32, it must be present
                "size": 0,

                // The metadata of this file, it must be present
                "meta": {
                    // This is a u64 in msgpack, it can be null / undefined
                    "last_update": 0,

                    // If this file has been used or not, it may be null / undefined, which is interpreted as false
                    "used": false,

                    // A note that a user has added to the file, or null / undefined. It may contain markdown syntax
                    "note": "note",

                    // The compression method that is applied to the file, can be null / undefined, which is interpreted as no compression
                    // possible values are: 
                    // "deflate": DEFLATE (used in .zip files)
                    // "gzip": glib's DEFLATE format (used in .gz files)
                    // null: (No compression)
                    "compress": "deflate",
                },
            }
        },
        // Metadata about this directory, it must be present but it can be an empty value
        "meta": {
            // A note that the user has added to the directory, can be undefined
            "note": "note",
        }
    }
}
```

```msgpack

```
