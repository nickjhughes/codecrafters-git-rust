use anyhow::Result;
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::io::Read;

use crate::util::{high_bit, parse_size};

#[derive(Debug)]
struct Object {
    ty: ObjectType,
    size: usize,
    content: Vec<u8>,
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

    std::fs::write("test.pack", input)?;

    let version = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    assert!(version == 2 || version == 3);
    let object_count = u32::from_be_bytes([input[8], input[9], input[10], input[11]]);

    let mut input = &input[12..];

    let mut objects = Vec::new();
    for _ in 0..object_count {
        let (rest, object) = Object::parse(input)?;
        input = rest;
        objects.push(object);
    }

    // Patch delta objects
    for object in objects.iter_mut() {
        match &object.ty {
            ObjectType::OfsDelta(offset) => {
                todo!("offset delta");
                // let offset = offset.unwrap();
            }
            ObjectType::RefDelta(hash) => {
                let hash = hash.as_ref().unwrap();
                let patched = patch_delta(&object.content)?;
            }
            _ => {}
        }
    }

    // All delta objects should be patched to regular objects at this point
    assert!(!objects
        .iter()
        .any(|o| matches!(o.ty, ObjectType::OfsDelta(_) | ObjectType::RefDelta(_))));

    Ok(())
}

#[derive(Debug, PartialEq)]
enum PatchInstruction {
    Copy { offset: usize, size: usize },
    Add { size: usize, data: Vec<u8> },
}

fn patch_delta(input: &[u8]) -> Result<Vec<u8>> {
    let (input, source_buf_len) = parse_size(input)?;
    let (input, target_buf_len) = parse_size(input)?;

    let mut rest = input;
    while !rest.is_empty() {
        let (remainder, instruction) = parse_patch_instruction(rest)?;
        rest = remainder;

        dbg!(&instruction);
    }

    todo!("apply patch instructions")
}

fn parse_patch_instruction(input: &[u8]) -> Result<(&[u8], PatchInstruction)> {
    match high_bit(input[0]) {
        true => {
            // Copy instruction
            let mut bytes_read = 1;
            let mut offset_bytes = [0, 0, 0, 0];
            let mut size_bytes = [0, 0, 0, 0];
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
            Ok((&input[1 + size..], PatchInstruction::Add { size, data }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_patch_instruction, Object, ObjectType, PatchInstruction};

    #[test]
    fn test_parse_object_header() {
        let data = &[0x9d, 0x0e];
        let (rest, (object_type, object_size)) = Object::parse_header(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(object_type, ObjectType::Commit);
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
                size: 2,
                data: vec![0x12, 0xab],
            }
        );
    }
}
