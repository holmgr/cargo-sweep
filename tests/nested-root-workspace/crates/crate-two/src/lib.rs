pub fn crate_two_string() -> String {
    "This is crate-two".to_string()
}

#[cfg(test)]
mod test {
    use super::crate_two_string;

    #[test]
    fn test() {
        assert_eq!("This is crate-two".to_string(), crate_two_string());
    }
}
