use anyhow::Result;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::{
    io::{Read, Write},
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
    time::SystemTime,
};

#[derive(Debug)]
pub enum Object {
    Blob(Vec<u8>),
    Tree(Vec<TreeEntry>),
    Commit(Commit),
}

#[derive(Debug)]
pub struct TreeEntry {
    mode: String,
    name: String,
    hash: String,
}

#[derive(Debug)]
pub struct Commit {
    tree_hash: String,
    parent_hash: Option<String>,
    message: String,
    timestamp: SystemTime,
}

impl Commit {
    fn new(tree_hash: &str, parent_hash: Option<&str>, message: &str) -> Self {
        Commit {
            tree_hash: tree_hash.to_owned(),
            parent_hash: parent_hash.map(|s| s.to_owned()),
            message: message.to_owned(),
            timestamp: SystemTime::now(),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut output = String::new();

        output.push_str("tree ");
        output.push_str(&self.tree_hash);
        output.push('\n');

        if let Some(parent_hash) = self.parent_hash.as_ref() {
            output.push_str("parent ");
            output.push_str(parent_hash);
            output.push('\n');
        }

        let timestamp_seconds = self
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        output.push_str(&format!(
            "author Author Name <author@example.com> {} +0000\n",
            timestamp_seconds
        ));
        output.push_str(&format!(
            "committer Committer Name <committer@example.com> {} +0000\n",
            timestamp_seconds,
        ));

        output.push('\n');
        output.push_str(&self.message);
        output.push('\n');

        output.as_bytes().to_vec()
    }

    fn encoded_len(&self) -> usize {
        self.encode().len()
    }
}

impl TreeEntry {
    fn new<P>(path: P) -> Result<Self>
    where
        P: Into<PathBuf>,
    {
        let path: PathBuf = path.into();

        let metadata = std::fs::metadata(&path)?;
        let mode = if metadata.is_dir() {
            "40000".to_owned()
        } else {
            format!("{:o}", metadata.permissions().mode())
        };

        let name = path.file_name().unwrap().to_str().unwrap().to_owned();

        let object = Object::new_from_path(path)?;
        let hash = object.hash();

        Ok(TreeEntry { mode, name, hash })
    }

    fn parse(input: &[u8]) -> Result<(&[u8], Self)> {
        let mut i = 0;
        while input[i] != b' ' {
            i += 1;
        }
        let mode = std::str::from_utf8(&input[0..i])?.to_owned();
        let input = &input[i + 1..];

        let mut i = 0;
        while input[i] != 0 {
            i += 1;
        }
        let name = std::str::from_utf8(&input[0..i])?.to_owned();
        let input = &input[i + 1..];

        let hash = hex::encode(&input[0..20]);
        let input = &input[20..];

        Ok((input, TreeEntry { mode, name, hash }))
    }

    fn encoded_len(&self) -> usize {
        // mode + space + name + \0 + SHA1 hash
        self.mode.len() + 1 + self.name.len() + 1 + 20
    }

    fn encode(&self) -> Vec<u8> {
        let mut output = Vec::new();

        output.write_all(self.mode.as_bytes()).unwrap();
        output.write_all(&[b' ']).unwrap();
        output.write_all(self.name.as_bytes()).unwrap();
        output.write_all(&[0]).unwrap();
        output.write_all(&hex::decode(&self.hash).unwrap()).unwrap();

        output
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
        } else if object_header.starts_with("commit") {
            todo!("parsing commit object")
        } else {
            anyhow::bail!("invalid object");
        }
    }

    /// Create a new commit object.
    pub fn new_commit(tree_hash: &str, parent_hash: Option<&str>, message: &str) -> Self {
        Object::Commit(Commit::new(tree_hash, parent_hash, message))
    }

    /// Create a new object from the given file or directory.
    pub fn new_from_path<P>(path: P) -> Result<Self>
    where
        P: Into<PathBuf>,
    {
        let path = path.into();
        let metadata = std::fs::metadata(&path)?;
        if metadata.is_dir() {
            let mut entries = Vec::new();
            for path in std::fs::read_dir(&path)? {
                let path = path.unwrap().path();
                if path.ends_with(".git") {
                    continue;
                }
                entries.push(TreeEntry::new(path)?);
            }
            entries.sort_by_key(|e| e.name.clone());
            Ok(Object::Tree(entries))
        } else {
            let object_content = std::fs::read(&path)?;
            Ok(Object::Blob(object_content))
        }
    }

    /// Add this object to the store.
    pub fn add(&self) -> Result<()> {
        let hash = self.hash();
        let mut path = PathBuf::from(".git/objects");
        path.push(&hash[0..2]);
        std::fs::create_dir_all(&path)?;
        path.push(&hash[2..40]);

        let file = std::fs::File::create(path)?;
        let mut encoder = flate2::write::ZlibEncoder::new(file, Compression::default());
        encoder.write_all(self.header().as_bytes())?;
        encoder.write_all(&[0])?;

        match self {
            Object::Blob(content) => {
                encoder.write_all(content)?;
            }
            Object::Tree(entries) => {
                for entry in entries.iter() {
                    encoder.write_all(&entry.encode())?;
                }
            }
            Object::Commit(commit) => {
                encoder.write_all(&commit.encode())?;
            }
        }

        Ok(())
    }

    /// Return the hash of this object.
    pub fn hash(&self) -> String {
        let mut hasher = Sha1::new();
        hasher.update(self.header().as_bytes());
        hasher.update([0]);

        match self {
            Object::Blob(content) => {
                hasher.update(content);
            }
            Object::Tree(entries) => {
                for entry in entries.iter() {
                    hasher.update(entry.encode());
                }
            }
            Object::Commit(commit) => {
                hasher.update(commit.encode());
            }
        }

        hex::encode(hasher.finalize())
    }

    /// Generate the header string for this object.
    fn header(&self) -> String {
        let mut header = String::new();
        match self {
            Object::Blob(content) => {
                header.push_str("blob ");
                header.push_str(&content.len().to_string());
            }
            Object::Tree(entries) => {
                header.push_str("tree ");
                let content_len = entries.iter().map(|e| e.encoded_len()).sum::<usize>();
                header.push_str(&content_len.to_string());
            }
            Object::Commit(commit) => {
                header.push_str("commit ");
                let content_len = commit.encoded_len();
                header.push_str(&content_len.to_string());
            }
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
            Object::Commit(commit) => {
                println!("{}", std::str::from_utf8(&commit.encode()).unwrap())
            }
        }
    }
}
