/// Formats a number of bytes into the closest binary SI unit, i.e KiB, MiB etc.
pub fn format_bytes(bytes: u64) -> String {
    let prefixes = ["bytes", "kiB", "MiB", "GiB", "TiB"];
    let mut bytes = bytes as f64;
    for prefix in prefixes.iter() {
        if bytes < 1024. {
            return format!("{:.2} {}", bytes, prefix);
        }
        bytes /= 1024.;
    }
    format!("{} TiB", bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024), "1 kiB");
        assert_eq!(format_bytes(1023), "1023 bytes");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500 MiB");
    }
}
