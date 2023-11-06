use anyhow::Result;
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, io::Read};

use crate::util::{high_bit, parse_size};

#[derive(Debug)]
struct PackedObject {
    ty: PackedObjectType,
    _size: usize,
    content: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum PackedObjectType {
    Commit,
    Tree,
    Blob,
    Tag,
    OfsDelta(Option<usize>),
    RefDelta(Option<String>),
}

// impl PackedObjectType {
//     fn from_content(content: &[u8]) -> Result<PackedObjectType> {
//         match std::str::from_utf8(&content[0..2])? {
//             "co" => Ok(PackedObjectType::Commit),
//             "tr" => Ok(PackedObjectType::Tree),
//             "blob" => Ok(PackedObjectType::Blob),
//             "tag" => Ok(PackedObjectType::Tag),
//             _ => anyhow::bail!("invalid object type"),
//         }
//     }
// }

impl PackedObject {
    fn parse_header(input: &[u8]) -> Result<(&[u8], (PackedObjectType, usize))> {
        let mut object_type = match (input[0] & 0x70) >> 4 {
            1 => PackedObjectType::Commit,
            2 => PackedObjectType::Tree,
            3 => PackedObjectType::Blob,
            4 => PackedObjectType::Tag,
            6 => PackedObjectType::OfsDelta(None),
            7 => PackedObjectType::RefDelta(None),
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
            PackedObjectType::OfsDelta(offset) => {
                let (rest, offset_value) = parse_size(input)?;
                *offset = Some(offset_value);
                rest
            }
            PackedObjectType::RefDelta(hash) => {
                *hash = Some(hex::encode(&input[0..20]));
                &input[20..]
            }
            _ => input,
        };

        Ok((input, (object_type, object_size)))
    }

    fn parse(input: &[u8]) -> Result<(&[u8], PackedObject)> {
        let (input, (object_type, object_size)) = PackedObject::parse_header(input)?;

        let mut decoder = ZlibDecoder::new(input);
        let mut content = Vec::new();
        decoder.read_to_end(&mut content)?;
        let input = &input[decoder.total_in() as usize..];

        Ok((
            input,
            PackedObject {
                ty: object_type,
                _size: object_size,
                content,
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

    let version = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    assert!(version == 2 || version == 3);
    let object_count = u32::from_be_bytes([input[8], input[9], input[10], input[11]]);

    let mut input = &input[12..];

    let mut objects: HashMap<String, PackedObject> = HashMap::new();
    for _ in 0..object_count {
        let (rest, mut packed_object) = PackedObject::parse(input)?;
        input = rest;

        match &packed_object.ty {
            PackedObjectType::OfsDelta(_) => {
                todo!("offset delta");
            }
            PackedObjectType::RefDelta(hash) => {
                let hash = hash.as_ref().unwrap();
                let ref_object = objects
                    .get(hash)
                    .expect("could not find delta reference object");
                packed_object.content = patch_delta(&packed_object.content, &ref_object.content)?;
                packed_object.ty = PackedObjectType::Blob;
            }
            _ => {}
        }

        let hash = {
            let mut hasher = Sha1::new();
            match packed_object.ty {
                PackedObjectType::Commit => hasher.update("commit "),
                PackedObjectType::Tree => hasher.update("tree "),
                PackedObjectType::Blob => hasher.update("blob "),
                PackedObjectType::Tag => hasher.update("tag "),
                _ => unreachable!(),
            }
            hasher.update(packed_object.content.len().to_string());
            hasher.update([0]);
            hasher.update(&packed_object.content);
            hex::encode(hasher.finalize())
        };

        objects.insert(hash, packed_object);
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
enum PatchInstruction {
    Copy { offset: usize, size: usize },
    Add { data: Vec<u8> },
}

fn patch_delta(input: &[u8], source: &[u8]) -> Result<Vec<u8>> {
    let (input, _source_buf_len) = parse_size(input)?;
    let (input, target_buf_len) = parse_size(input)?;

    let mut result = Vec::with_capacity(target_buf_len);

    let mut rest = input;
    while !rest.is_empty() {
        let (remainder, instruction) = parse_patch_instruction(rest)?;
        rest = remainder;

        match instruction {
            PatchInstruction::Copy { offset, size } => {
                result.extend(&source[offset..offset + size]);
            }
            PatchInstruction::Add { data } => {
                result.extend(data);
            }
        }
    }

    Ok(result)
}

fn parse_patch_instruction(input: &[u8]) -> Result<(&[u8], PatchInstruction)> {
    match high_bit(input[0]) {
        true => {
            // Copy instruction
            let mut bytes_read = 1;
            let mut offset_bytes = [0, 0, 0, 0];
            let mut size_bytes = [0, 0, 0, 0];
            #[allow(clippy::needless_range_loop)]
            for i in 0..4 {
                // Offset bytes
                if input[0] & (1 << i) != 0 {
                    offset_bytes[i] = input[bytes_read];
                    bytes_read += 1;
                }
            }
            for i in 4..7 {
                // Size bytes
                if input[0] & (1 << i) != 0 {
                    size_bytes[i - 4] = input[bytes_read];
                    bytes_read += 1;
                }
            }

            let instruction = PatchInstruction::Copy {
                offset: u32::from_le_bytes(offset_bytes) as usize,
                size: u32::from_le_bytes(size_bytes) as usize,
            };

            Ok((&input[bytes_read..], instruction))
        }
        false => {
            // Add instruction
            let size = (input[0] & 0x7f) as usize;
            assert!(size > 0, "invalid instruction");
            let data = input[1..1 + size].to_vec();
            Ok((&input[1 + size..], PatchInstruction::Add { data }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_patch_instruction, PackedObject, PackedObjectType, PatchInstruction};

    #[test]
    fn test_parse_object_header() {
        let data = &[0x9d, 0x0e];
        let (rest, (object_type, object_size)) = PackedObject::parse_header(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(object_type, PackedObjectType::Commit);
        assert_eq!(object_size, 237);
    }

    #[test]
    fn test_parse_copy_instruction() {
        let data = &[0x85, 0x12, 0xab];
        let (rest, instruction) = parse_patch_instruction(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(
            instruction,
            PatchInstruction::Copy {
                offset: 11206674,
                size: 0,
            }
        );
    }

    #[test]
    fn test_parse_add_instruction() {
        let data = &[0x02, 0x12, 0xab];
        let (rest, instruction) = parse_patch_instruction(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(
            instruction,
            PatchInstruction::Add {
                data: vec![0x12, 0xab],
            }
        );
    }
}
