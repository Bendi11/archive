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
- ENC: 7,
- USED: 8,
- COMPRESSMETHOD: 9,

```
Header: Array (root) [
    <Meta>,
    &[u8 ; 12]: Nonce counter
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
    Integer ENC: u64 (nonce)
]

Meta: Map {
    Integer USED: Boolean (if the file has been used),
    Integer NOTE: String (note),
    Integer NAME: String (name),
}
```

The compression method is a string with the following format:
> - "none": No compression
> - "{QUALITY}-{METHOD}": QUALITY can be any of: 
>   - "high", "medium", "fast"
>  And METHOD can be any one of: 
>   - "gzip", "deflate"