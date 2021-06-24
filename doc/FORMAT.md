# Archive Format: 
A full archive is made up of a singly linked list of data, with data being different based on a header

## Header Format:
The initial 4 bits represent what kind of entry this is:
### 0: FILE => 
This is a file entry, the next 4 bits are the compression scheme: 
>
> - `0` => No compression applied to file
> - `1` => DEFLATE compression applied to file

---

After the initial header byte, the format is: 
\[ file data size (u32) \] \[ file name length (u8) \] \[ file name (variable) \] \[ file flags (u8) \]
The `file flags` item is a bitfield that will add data depending on the set flags:

> 
> - `00000010` => This just means that the file has been used or watched before
> - `00000100` => \[ note len (u16) \] \[ note (variable) \]
> ...

After the flags data, the file data follows (with a length as specified in the header):
\[ file data (variable ) \]

-----

### 1: DIR => 
This is a directory start, directories are entered with the `DIR` flag and exited with the

`ENDDIR` flag. The following 4 bits are directory bitfield flags, with different data based on the flags: 

> - `0001` => \[ dir note len (u16) \] \[ dir note (variable) \]
> ...

\[ dir name len(u8) \] \[ dir name (variable) \] 

-----

### 2: ENDDIR => 
This means that a directory should be popped from the current path. For example, if two DIR headers had just been encountered, with

names `usr` and `bin` respectively, the path would be `usr/bin`. When an ENDDIR header is read, then the path becomes

`usr/`. The following 4 bits are currently unused

-----

### 3: META => 
The `META` header flag indicates that the following data is metadata about the archive. 

The following 4 bits indicate what the `META` field indicates:
> - `0` => This means that the metadata is the archive name, so the following data is: 
>
>   \[ archive name len(u8) \] \[ archive name (variable) \]

----------

# Example Structure
The following is an example archive for an archive named `music` with one folder `men at work`, that contains the file `land down under.mp3` with
no compression method: 
```
[ 3 (META) (u4) ] [ 0 (METANAME) (u4) ] [ 5 (METANAME LEN) (u8) ] [ 'music' ] 


[ 1 (DIR) (u4) ] [ 0 (NO DIR FLAGS) (u4) ] [ 11 (DIR NAME LEN) (u8) ] [ 'men at work' ] 


[ 0 (FILE) (u4) ] [ 0 (NO FILE COMPRESSION) (u4) ] [ 1000 (FILE SIZE) (u32) ] 

[ 19 (FILE NAME LEN) (u8) ] [ 'land down under.mp3' ] [ 0 (NO FILE FLAGS) (u8) ]

[ FILE DATA ] 

[ 2 (ENDDIR) (u4) ] [ 0 (UNUSED) (u4) ]
```
