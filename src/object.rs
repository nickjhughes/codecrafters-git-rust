use anyhow::Result;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Debug)]
pub enum Object {
    Blob(Vec<u8>),
    Tree(Vec<TreeEntry>),
}

#[derive(Debug)]
pub struct TreeEntry {
    name: String,
}

impl TreeEntry {
    fn parse(input: &[u8]) -> Result<(&[u8], Self)> {
        let mut i = 0;
        while input[i] != b' ' {
            i += 1;
        }
        let _mode = std::str::from_utf8(&input[0..i])?;
        let input = &input[i + 1..];

        let mut i = 0;
        while input[i] != 0 {
            i += 1;
        }
        let name = std::str::from_utf8(&input[0..i])?.to_owned();
        let input = &input[i + 1..];

        let _hash = hex::encode(&input[0..20]);
        let input = &input[20..];

        Ok((input, TreeEntry { name }))
    }
}

impl Object {
    /// Parse an object from the store.
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
        } else if object_header.starts_with("tree") {
            let object_size = object_header[5..].parse::<usize>()?;
            assert_eq!(object_content.len(), object_size);

            let mut rest = object_content;
            let mut entries = Vec::new();
            while !rest.is_empty() {
                let (remainder, entry) = TreeEntry::parse(rest)?;
                rest = remainder;
                entries.push(entry);
            }

            Ok(Object::Tree(entries))
        } else {
            todo!("non-blob/tree object");
        }
    }

    /// Create a new object from the given file.
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: Into<PathBuf>,
    {
        let object_content = std::fs::read(path.into())?;
        Ok(Object::Blob(object_content))
    }

    /// Add this object to the store.
    pub fn add(&self) -> Result<()> {
        let hash = self.hash();
        let mut path = PathBuf::from(".git/objects");
        path.push(&hash[0..2]);
        std::fs::create_dir_all(&path)?;
        path.push(&hash[2..40]);

        match self {
            Object::Blob(content) => {
                let file = std::fs::File::create(path)?;
                let mut encoder = flate2::write::ZlibEncoder::new(file, Compression::default());
                encoder.write_all(self.header().as_bytes())?;
                encoder.write_all(&[0])?;
                encoder.write_all(content)?;
            }
            Object::Tree(_) => todo!(),
        }

        Ok(())
    }

    /// Return the hash of this object.
    pub fn hash(&self) -> String {
        match self {
            Object::Blob(content) => {
                let mut hasher = Sha1::new();
                hasher.update(self.header().as_bytes());
                hasher.update([0]);
                hasher.update(content);
                hex::encode(hasher.finalize())
            }
            Object::Tree(_) => todo!(),
        }
    }

    /// Generate the header string for this object.
    fn header(&self) -> String {
        let mut header = String::new();
        match self {
            Object::Blob(content) => {
                header.push_str("blob ");
                header.push_str(&content.len().to_string());
            }
            Object::Tree(_) => todo!(),
        }
        header
    }

    /// Print the contents of this object.
    pub fn print(&self) {
        match self {
            Object::Blob(content) => {
                print!(
                    "{}",
                    std::str::from_utf8(content).expect("non utf-8 blob contents")
                );
            }
            Object::Tree(entries) => {
                for entry in entries.iter() {
                    println!("{}", entry.name);
                }
            }
        }
    }
}
