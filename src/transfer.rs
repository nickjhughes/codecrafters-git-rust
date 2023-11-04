use anyhow::Result;
use reqwest::StatusCode;

#[derive(Debug)]
struct Ref<'resp> {
    name: &'resp str,
    hash: &'resp str,
}

pub fn clone(repo_url: reqwest::Url) -> Result<()> {
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

    let content = resp.text()?;

    let first_five_bytes = &content.as_bytes()[0..5];
    assert!(&first_five_bytes[0..4].iter().all(|ch| {
        ch.is_ascii_hexdigit() && (!ch.is_ascii_alphabetic() || ch.is_ascii_lowercase())
    }));
    assert!(first_five_bytes[4] == b'#');

    let pkt_lines = content
        .split_once('#')
        .unwrap()
        .1
        .lines()
        .collect::<Vec<&str>>();
    assert_eq!(*pkt_lines.first().unwrap(), " service=git-upload-pack");
    assert_eq!(*pkt_lines.last().unwrap(), "0000");

    let mut refs = Vec::new();
    for (i, ref_line) in pkt_lines.iter().skip(1).enumerate() {
        if *ref_line == "0000" {
            continue;
        }

        let ref_info = if let Some((ref_info, _)) = ref_line.split_once('\0') {
            ref_info
        } else {
            ref_line
        };
        let ref_info = if i == 0 {
            ref_info.trim_start_matches("0000")
        } else {
            ref_info
        };
        let (hash, name) = ref_info.split_once(' ').unwrap();
        let (_, hash) = hash.split_at(4);
        refs.push(Ref { hash, name });
    }
    let head_ref = refs.iter().find(|r| r.name == "HEAD").unwrap();
    dbg!(&head_ref);

    Ok(())
}
