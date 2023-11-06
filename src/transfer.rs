use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use flate2::Compression;
use reqwest::StatusCode;

use crate::{pack::parse_pack_file, util::parse_packet_lines};

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

pub fn get_refs(repo_url: &reqwest::Url) -> Result<(Vec<Ref>, Vec<String>)> {
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

pub fn clone(repo_url: reqwest::Url, directory: PathBuf) -> Result<()> {
    let (refs, _capabilities) = get_refs(&repo_url)?;
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
    let objects = parse_pack_file(pack_file)?;

    fs::create_dir_all(&directory)?;

    for (hash, object) in objects.iter() {
        let mut path = PathBuf::from(&directory);
        path.push(".git/objects");
        path.push(&hash[0..2]);
        fs::create_dir_all(&path)?;
        path.push(&hash[2..40]);

        let file = fs::File::create(path)?;
        let mut encoder = flate2::write::ZlibEncoder::new(file, Compression::default());
        encoder.write_all(&object.encode())?;
    }

    let mut path = PathBuf::from(&directory);
    path.push(".git/refs");
    fs::create_dir_all(path)?;

    let mut path = PathBuf::from(&directory);
    path.push(".git/HEAD");
    fs::write(path, "ref: refs/heads/master\n")?;

    Ok(())
}
