use anyhow::Result;

/// Parse packet line data until a flush packet ("0000") or a pack file is found
pub fn parse_packet_lines(input: &[u8]) -> Result<(&[u8], Vec<&[u8]>)> {
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

/// Parse a variable-length encoded integer.
pub fn parse_size(input: &[u8]) -> Result<(&[u8], usize)> {
    let mut i = 0;
    let mut value: usize = (input[i] as usize) & 0x0f;
    while high_bit(input[i]) {
        i += 1;
        value |= ((input[i] as usize) & 0x7f) << 7;
    }
    let input = &input[i + 1..];
    Ok((input, value))
}

pub fn high_bit(byte: u8) -> bool {
    (byte & 0x80) >> 7 != 0
}

#[cfg(test)]
mod tests {
    use super::parse_packet_lines;

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
}
