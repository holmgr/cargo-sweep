/// Formats a number of bytes into the closest binary SI unit, i.e KiB, MiB etc.
pub fn format_bytes(bytes: u64) -> String {
    let prefixes = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut bytes = bytes as f64;
    for prefix in prefixes.iter() {
        if bytes < 1024. {
            return format!("{bytes:.2} {prefix}");
        }
        bytes /= 1024.;
    }
    format!("{bytes} TiB")
}

/// Like [format_bytes], but with a special case for formatting `0` as `"nothing"`.
/// With `--recursive`, this helps visually distinguish folders without outdated artifacts.
/// See [#93](https://github.com/holmgr/cargo-sweep/issues/93).
pub fn format_bytes_or_nothing(bytes: u64) -> String {
    match bytes {
        0 => "nothing".to_string(),
        _ => format_bytes(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1023), "1023.00 B");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500.00 MiB");

        // Assert that the human-size crate can parse the output from cargo-sweep
        assert_eq!("1.00 B".parse::<human_size::Size>().unwrap().to_bytes(), 1);
        assert_eq!(
            "1.00 KiB".parse::<human_size::Size>().unwrap().to_bytes(),
            1024
        );
        assert_eq!(
            "1.00 MiB".parse::<human_size::Size>().unwrap().to_bytes(),
            1024 * 1024
        );
        assert_eq!(
            "1.00 GiB".parse::<human_size::Size>().unwrap().to_bytes(),
            1024 * 1024 * 1024
        );
        assert_eq!(
            "1.00 TiB".parse::<human_size::Size>().unwrap().to_bytes(),
            1024 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_format_bytes_or_nothing() {
        assert_eq!(format_bytes_or_nothing(0), "nothing");

        // Copy-pasted some non-zero values from `format_bytes` tests to test that the output is identical.
        assert_eq!(format_bytes(1024), format_bytes_or_nothing(1024));
        assert_eq!(format_bytes(1023), format_bytes_or_nothing(1023));
        assert_eq!(
            format_bytes(500 * 1024 * 1024),
            format_bytes_or_nothing(500 * 1024 * 1024)
        );
    }
}
