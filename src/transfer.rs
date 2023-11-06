use anyhow::Result;
use flate2::read::ZlibDecoder;
use reqwest::StatusCode;
use sha1::{Digest, Sha1};
use std::io::Read;

#[derive(Debug)]
pub struct Ref {
    name: String,
    hash: String,
}

impl Ref {
    pub fn print(&self) {
        println!("{} {}", self.hash, self.name);
    }
}

/// Parse a variable-length encoded integer.
fn parse_size(input: &[u8]) -> Result<(&[u8], usize)> {
    let mut i = 0;
    let mut value: usize = (input[i] as usize) & 0x0f;
    while high_bit(input[i]) {
        i += 1;
        value |= ((input[i] as usize) & 0x7f) << 7;
    }
    let input = &input[i + 1..];
    Ok((input, value))
}

fn high_bit(byte: u8) -> bool {
    (byte & 0x80) >> 7 != 0
}

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
    fn parse(input: &[u8]) -> Result<(&[u8], Object)> {
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

/// Parse packet line data until a flush packet ("0000") or a pack file is found
fn parse_packet_lines(input: &[u8]) -> Result<(&[u8], Vec<&[u8]>)> {
    let mut rest = input;

    let mut lines = Vec::new();
    loop {
        if &rest[0..4] == b"PACK" {
            // Found a pack file, assume rest of response is that file
            lines.push(rest);
            rest = &[];
            break;
        }

        let line_length = u16::from_str_radix(std::str::from_utf8(&rest[0..4])?, 16)? as usize;
        rest = &rest[4..];

        if line_length == 0 {
            // Flush packet
            break;
        }

        let line = if rest[line_length - 5] == b'\n' {
            // Ignore trailing newlines but don't require them
            &rest[0..line_length - 5]
        } else {
            &rest[0..line_length - 4]
        };
        lines.push(line);
        rest = &rest[line_length - 4..];
    }

    Ok((rest, lines))
}

pub fn ls_remote(repo_url: &reqwest::Url) -> Result<(Vec<Ref>, Vec<String>)> {
    let client = reqwest::blocking::Client::new();

    let request = client.get(format!("{}/info/refs?service=git-upload-pack", repo_url));
    let resp = request.send()?;
    let resp_headers = resp.headers();

    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_MODIFIED);
    assert!(resp_headers.get("content-type").is_some());
    assert_eq!(
        resp_headers.get("content-type").unwrap().to_str().unwrap(),
        "application/x-git-upload-pack-advertisement"
    );

    let content = resp.bytes()?;
    let (rest, lines) = parse_packet_lines(&content)?;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], b"# service=git-upload-pack");

    let (rest, ref_lines) = parse_packet_lines(rest)?;
    assert!(rest.is_empty());

    let mut refs = Vec::new();
    let mut capabilities = Vec::new();
    for (i, ref_line) in ref_lines.iter().enumerate() {
        let ref_line_str = std::str::from_utf8(ref_line)?;
        let ref_info = if let Some((ref_info, capabilities_str)) = ref_line_str.split_once('\0') {
            if i == 0 {
                capabilities.extend(capabilities_str.split_whitespace().map(|s| s.to_owned()));
            } else {
                anyhow::bail!("capabilities should only accompany first ref");
            }

            ref_info
        } else {
            ref_line_str
        };
        let (hash, name) = ref_info.split_once(' ').unwrap();
        refs.push(Ref {
            hash: hash.to_owned(),
            name: name.to_owned(),
        });
    }

    Ok((refs, capabilities))
}

pub fn clone(repo_url: reqwest::Url) -> Result<()> {
    let (refs, _capabilities) = ls_remote(&repo_url)?;
    let wanted_refs = refs
        .iter()
        .filter(|r| r.name.starts_with("refs/heads"))
        .collect::<Vec<_>>();

    // Send want request
    const MY_CAPABILITIES: [&str; 1] = ["agent=git/1.8.1"];
    let mut request_body = String::new();
    for (i, ref_) in wanted_refs.iter().enumerate() {
        let mut line = String::new();
        line.push_str("want ");
        line.push_str(&ref_.hash);
        if i == 0 {
            line.push('\0');
            line.push_str(&MY_CAPABILITIES.join(" "));
        }
        let line_length = line.len() + 4 + 1; // add 4 for length string and 1 for trailing newline
        assert!(line_length <= u16::MAX as usize);
        request_body.push_str(&format!("{:04x}{}\n", line_length, line));
    }
    request_body.push_str("0000");
    request_body.push_str("0009done\n");
    request_body.push_str("0000");

    let client = reqwest::blocking::Client::new();
    let request = client
        .post(format!("{}/git-upload-pack", repo_url))
        .header("content-type", "application/x-git-upload-pack-request")
        .body(request_body);
    let resp = request.send()?;
    let resp_headers = resp.headers();

    assert!(resp.status().is_success());
    assert!(resp_headers.get("content-type").is_some());
    assert_eq!(
        resp_headers.get("content-type").unwrap().to_str().unwrap(),
        "application/x-git-upload-pack-result"
    );

    let data = &resp.bytes()?[..];
    let (rest, lines) = parse_packet_lines(data)?;
    assert!(rest.is_empty());
    assert_eq!(lines[0], b"NAK");
    let pack_file = lines[1];
    parse_pack_file(pack_file)?;

    Ok(())
}

fn parse_pack_file(input: &[u8]) -> Result<()> {
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
    use super::{parse_packet_lines, Object, ObjectType};

    #[test]
    fn test_parse_packet_lines() {
        let data = b"001e# service=git-upload-pack\n0000";
        let (rest, lines) = parse_packet_lines(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(
            std::str::from_utf8(lines[0]).unwrap(),
            "# service=git-upload-pack"
        );
    }

    #[test]
    fn test_parse_object() {
        let data = &[0x9d, 0x0e];
        let (rest, object) = Object::parse(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(object.ty, ObjectType::Commit);
        assert_eq!(object.size, 237);
    }
}
