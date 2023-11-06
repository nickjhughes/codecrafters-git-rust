use anyhow::Result;
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::io::Read;

use crate::util::{high_bit, parse_size};

#[derive(Debug)]
struct Object {
    ty: ObjectType,
    size: usize,
    _content: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum ObjectType {
    Commit,
    Tree,
    Blob,
    Tag,
    OfsDelta(Option<isize>),
    RefDelta(Option<String>),
}

impl Object {
    fn parse_header(input: &[u8]) -> Result<(&[u8], (ObjectType, usize))> {
        let mut object_type = match (input[0] & 0x70) >> 4 {
            1 => ObjectType::Commit,
            2 => ObjectType::Tree,
            3 => ObjectType::Blob,
            4 => ObjectType::Tag,
            6 => ObjectType::OfsDelta(None),
            7 => ObjectType::RefDelta(None),
            _ => anyhow::bail!("invalid object type"),
        };

        let (input, object_size) = if high_bit(input[0]) {
            let (rest, object_size_extra) = parse_size(&input[1..])?;
            let object_size = ((input[0] & 0x0f) as usize) | (object_size_extra << 4);
            (rest, object_size)
        } else {
            (&input[1..], (input[0] & 0x0f) as usize)
        };

        let input = match &mut object_type {
            ObjectType::OfsDelta(offset) => {
                let (rest, offset_value) = parse_size(input)?;
                *offset = Some(-(offset_value as isize));
                rest
            }
            ObjectType::RefDelta(hash) => {
                *hash = Some(hex::encode(&input[0..20]));
                &input[20..]
            }
            _ => input,
        };

        Ok((input, (object_type, object_size)))
    }

    fn parse(input: &[u8]) -> Result<(&[u8], Object)> {
        let (input, (object_type, object_size)) = Object::parse_header(input)?;

        let mut decoder = ZlibDecoder::new(input);
        let mut content = Vec::new();
        decoder.read_to_end(&mut content)?;
        let input = &input[decoder.total_in() as usize..];

        Ok((
            input,
            Object {
                ty: object_type,
                size: object_size,
                _content: content,
            },
        ))
    }
}

pub fn parse_pack_file(input: &[u8]) -> Result<()> {
    assert_eq!(&input[0..4], b"PACK");

    let checksum = {
        let mut hasher = Sha1::new();
        hasher.update(&input[0..input.len() - 20]);
        hex::encode(hasher.finalize())
    };
    assert_eq!(
        hex::encode(&input[input.len() - 20..]),
        checksum,
        "pack file checksum failure"
    );

    std::fs::write("test.pack", input)?;

    let version = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    assert!(version == 2 || version == 3);
    let object_count = u32::from_be_bytes([input[8], input[9], input[10], input[11]]);

    let mut input = &input[12..];

    for _ in 0..object_count {
        let (rest, object) = Object::parse(input)?;
        let compressed_size = input.len() - rest.len();
        println!(
            "Parsed {:?} of size {} from {} bytes",
            object.ty, object.size, compressed_size
        );

        input = rest;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Object, ObjectType};

    #[test]
    fn test_parse_object_header() {
        let data = &[0x9d, 0x0e];
        let (rest, (object_type, object_size)) = Object::parse_header(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(object_type, ObjectType::Commit);
        assert_eq!(object_size, 237);
    }
}
