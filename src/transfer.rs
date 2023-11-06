use anyhow::Result;
use reqwest::StatusCode;

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

/// Parse pkt-line data until a flust-pkt ("0000") is found
fn parse_pktlines(input: &[u8]) -> Result<(&[u8], Vec<&[u8]>)> {
    let mut rest = input;

    let mut lines = Vec::new();
    loop {
        let line_length = u16::from_str_radix(std::str::from_utf8(&rest[0..4])?, 16)? as usize;
        rest = &rest[4..];

        if line_length == 0 {
            // flush-pkt
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

pub fn ls_remote(repo_url: reqwest::Url) -> Result<(Vec<Ref>, Vec<String>)> {
    let client = reqwest::blocking::Client::new();

    let request = client.get(format!("{}/info/refs?service=git-upload-pack", repo_url));
    let resp = request.send()?;
    let resp_headers = resp.headers();

    assert!(resp_headers.get("content-type").is_some());
    assert_eq!(
        resp_headers.get("content-type").unwrap().to_str().unwrap(),
        "application/x-git-upload-pack-advertisement"
    );
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_MODIFIED);

    let content = resp.bytes()?;
    let (rest, lines) = parse_pktlines(&content)?;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], b"# service=git-upload-pack");

    let (rest, ref_lines) = parse_pktlines(rest)?;
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
        let (_, hash) = hash.split_at(4);
        refs.push(Ref {
            hash: hash.to_owned(),
            name: name.to_owned(),
        });
    }

    Ok((refs, capabilities))
}

pub fn clone(repo_url: reqwest::Url) -> Result<()> {
    let (refs, _capabilities) = ls_remote(repo_url)?;

    let head_ref = refs.iter().find(|r| r.name == "HEAD").unwrap();
    dbg!(&head_ref);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_pktlines;

    #[test]
    fn test_parse_pktlines() {
        let data = b"001e# service=git-upload-pack\n0000";
        let (rest, lines) = parse_pktlines(data).unwrap();
        assert!(rest.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(
            std::str::from_utf8(lines[0]).unwrap(),
            "# service=git-upload-pack"
        );
    }
}
