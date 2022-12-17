fn main() {
    println!("{}", bin_string());
}

fn bin_string() -> String {
    "This is bin-crate".to_string()
}

#[cfg(test)]
mod test {
    use super::bin_string;

    #[test]
    fn test() {
        assert_eq!("This is bin-crate".to_string(), bin_string());
    }
}
