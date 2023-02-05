pub fn crate_one_string() -> String {
    "This is crate-one".to_string()
}

#[cfg(test)]
mod test {
    use super::crate_one_string;

    #[test]
    fn test() {
        assert_eq!("This is crate-one".to_string(), crate_one_string());
    }
}
