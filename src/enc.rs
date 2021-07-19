use aes::{Aes128, cipher::{BlockEncrypt, BlockDecrypt, NewBlockCipher, generic_array::GenericArray}};
use aes::cipher::consts::U16;

use crate::ar::BarResult;

use std::io::{Read, Seek, Write, SeekFrom};

/// Encrypt a reader, writing the encrypted bytes to a writer
pub fn encrypt(reader: &mut impl Read, writer: &mut impl Write, key: &[u8 ; 16]) -> BarResult<()> {
    let key = GenericArray::from_slice(key);
    let cipher = Aes128::new(key);
    let mut buf = GenericArray::<u8, U16>::default();
    
    loop {
        let read = reader.read(&mut buf)?;
        if read < 16 {
            writer.write_all(&buf[0..read])?;
            break
        } else {
            cipher.encrypt_block(&mut buf);
            writer.write_all(&buf)?;
        }
    }
    Ok(())
}

/// Decrypt a reader, writing decrypted bytes to a writer
pub fn decrypt(reader: &mut impl Read, writer: &mut impl Write, key: &[u8 ; 16]) -> BarResult<()> {
    let key = GenericArray::from_slice(key);
    let cipher = Aes128::new(key);
    let mut buf = GenericArray::<u8, U16>::default();

    loop {
        let read = reader.read(&mut buf).unwrap();
        if read < 16 {
            writer.write_all(&buf[0..read]).unwrap();
            break
        } else {
            cipher.decrypt_block(&mut buf);
            writer.write_all(&buf).unwrap();
        }
    }
    Ok(())
}

/// Encrypt a buffer in place
pub fn encrypt_in_place(plaintxt: &mut (impl Read + Write + Seek), key: &[u8 ; 16]) -> BarResult<()> {
    let key = GenericArray::from_slice(key);
    let cipher = Aes128::new(key);
    let mut buf = GenericArray::<u8, U16>::default();

    loop {
        let read = plaintxt.read(&mut buf)?;
        if read < 16 {
            plaintxt.seek(SeekFrom::Current(-(read as i64)))?;
            plaintxt.write_all(&buf[0..read])?;
            break
        } else {
            plaintxt.seek(SeekFrom::Current(-16))?;
            cipher.encrypt_block(&mut buf);
            plaintxt.write_all(&buf)?;
        }

    }
    Ok(())
}

/// Decrypt a buffer in place
pub fn decrypt_in_place(ciphertxt: &mut (impl Read + Write + Seek), key: &[u8 ; 16]) -> BarResult<()> {
    let key = GenericArray::from_slice(key);
    let cipher = Aes128::new(key);
    let mut buf = GenericArray::<u8, U16>::default();

    loop {
        let read = ciphertxt.read(&mut buf)?;
        if read < 16 {
            ciphertxt.seek(SeekFrom::Current(-(read as i64)))?;
            ciphertxt.write_all(&buf[0..read])?;
            break
        } else {
            ciphertxt.seek(SeekFrom::Current(-16))?;
            cipher.decrypt_block(&mut buf);
            ciphertxt.write_all(&buf)?;
        }

    }
    Ok(())
}