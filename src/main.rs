pub mod archive;
pub mod compress;

use std::io::Cursor;

use archive::LzSS;


const TXT: &[u8] = include_bytes!("./test.txt");

fn main() {
    let mut ar = LzSS::<_, u16>::new(Cursor::new(TXT)); 
    let data = ar.debug_compress();
    std::fs::write("out_string.txt", &data[..]).unwrap();
    //println!("{}", data);
    let data = ar.compress(); 
    std::fs::write("out.txt", &data[..]).unwrap();
    let mut decar = LzSS::<_, u16>::new(Cursor::new(data));

    println!("{}", String::from_utf8(decar.decompress()).unwrap())
    //println!("{:#?}", decar.decompress())
}
