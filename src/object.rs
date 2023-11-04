use anyhow::Result;
use std::{io::Read, path::PathBuf};

#[derive(Debug)]
pub enum Object {
    Blob(Vec<u8>),
}

impl Object {
    pub fn parse<P>(path: P) -> Result<Self>
    where
        P: Into<PathBuf>,
    {
        let compressed = std::fs::read(path.into())?;
        let mut decoder = flate2::read::ZlibDecoder::new(&compressed[..]);
        let mut content = Vec::new();
        decoder.read_to_end(&mut content)?;

        let (object_header, object_content) = {
            let mut i = 0;
            while content[i] != 0 {
                i += 1;
            }
            (std::str::from_utf8(&content[0..i])?, &content[i + 1..])
        };

        if object_header.starts_with("blob") {
            let object_size = object_header[5..].parse::<usize>()?;
            assert_eq!(object_content.len(), object_size);
            Ok(Object::Blob(object_content.to_vec()))
        } else {
            todo!("non-blob object");
        }
    }

    pub fn print(&self) {
        match self {
            Object::Blob(content) => {
                print!(
                    "{}",
                    std::str::from_utf8(content).expect("non utf-8 blob contents")
                );
            }
        }
    }
}
