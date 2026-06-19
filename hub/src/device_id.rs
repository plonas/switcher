use anyhow::{Result, anyhow, bail};

pub fn format_device_id(device_id: &[u8; 8]) -> String {
    let mut output = String::with_capacity(16);

    for byte in device_id {
        output.push(hex_digit(byte >> 4));
        output.push(hex_digit(byte & 0x0f));
    }

    output
}

pub fn parse_device_id(input: &str) -> Result<[u8; 8]> {
    let trimmed = input.trim();
    if trimmed.len() != 16 {
        bail!(
            "device_id must be exactly 16 lowercase hex characters, got {}",
            trimmed.len()
        );
    }

    let mut device_id = [0_u8; 8];
    let bytes = trimmed.as_bytes();

    for (index, chunk) in bytes.chunks_exact(2).enumerate() {
        let high = parse_hex_nibble(chunk[0])?;
        let low = parse_hex_nibble(chunk[1])?;
        device_id[index] = (high << 4) | low;
    }

    Ok(device_id)
}

fn parse_hex_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(anyhow!("device_id contains non-lowercase-hex character")),
    }
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex digit out of range"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_roundtrip() {
        let device_id = [0x12, 0x34, 0xab, 0xcd, 0xef, 0x00, 0x01, 0x02];
        let encoded = format_device_id(&device_id);
        assert_eq!(encoded, "1234abcdef000102");
        assert_eq!(parse_device_id(&encoded).unwrap(), device_id);
    }

    #[test]
    fn parse_rejects_invalid_length() {
        assert!(parse_device_id("abcd").is_err());
    }
}
