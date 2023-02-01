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
}
