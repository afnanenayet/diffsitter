/// Truncate a string down to `width` using some fill string.
///
/// This will return the string as-is if it's shorter than the truncation length. That will
/// allocate a new string, because of this function's type signature.
///
/// # Arguments
///
/// - s: The input string that might be truncated.
/// - width: The maximum width of the string.
/// - fill: The placeholder string to use to represent the middle of the string, if it's truncated.
///
/// # Examples
///
/// ```rust
/// # use libdiffsitter::string_utils::truncate_str;
/// let input_str = "hello, world!";
/// let result = truncate_str(&input_str, 7, "...");
/// assert_eq!(result, "he...d!");
/// ```
///
/// # Panics
///
/// If the `fill` string is longer than the provided `width`.
pub fn truncate_str(s: &str, width: usize, fill: &str) -> String {
    if fill.len() > width {
        panic!(
            "The provided fill string (len: {}) is longer than the truncation width ({})",
            fill.len(),
            width
        );
    }
    if s.len() <= width {
        return s.into();
    }
    // We want to take roughly an equal amount from the front and back of the string.
    let length_to_take = (width - fill.len()) / 2;
    // Index to take from for the latter half of the string.
    let end_idx = s.len() - length_to_take;
    format!("{}{}{}", &s[..length_to_take], fill, &s[end_idx..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_str_eq;
    use rstest::*;

    #[rstest]
    #[case("12345", 4, "..", "1..5")]
    #[case("12345", 4, "||", "1||5")]
    #[case("12345", 5, "...", "12345")]
    #[case("short", 1000, "..", "short")]
    #[case("/some/large/path", 10, "..", "/som..path")]
    fn test_truncate_str(
        #[case] input_str: &str,
        #[case] width: usize,
        #[case] fill: &str,
        #[case] expected: &str,
    ) {
        let actual = truncate_str(&input_str, width, &fill);
        assert_str_eq!(actual, expected);
    }

    #[test]
    #[should_panic]
    fn test_bad_fill_length() {
        truncate_str(".", 1, "ahh too long!");
    }
}
