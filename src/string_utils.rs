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

/// Create a string from multiple objects that support `AsRef<str>`.
///
/// This was lifted from
/// [concat-string](https://github.com/FaultyRAM/concat-string/blob/942a4aa8244d5ff00dd9b9a34ecd0484feaf9a7f/src/lib.rs#L69C1-L79C2)
/// and incorporates a [proposed PR](https://github.com/FaultyRAM/concat-string/pull/1.) to add
/// support for trailing commas.
///
/// This is supposed to be pretty efficient, compared to all of the string concatenation techniques
/// in Rust according to some [benchmarks](https://github.com/hoodie/concatenation_benchmarks-rs).
#[macro_export]
macro_rules! concat_string {
    () => { String::with_capacity(0) };
    ($($s:expr),+ $(,)?) => {{
        use std::ops::AddAssign;
        let mut len = 0;
        $(len.add_assign(AsRef::<str>::as_ref(&$s).len());)+
        let mut buf = String::with_capacity(len);
        $(buf.push_str($s.as_ref());)+
        buf
    }};
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

    // Concat string tests were copied from
    // https://github.com/FaultyRAM/concat-string/blob/942a4aa8244d5ff00dd9b9a34ecd0484feaf9a7f/src/lib.rs

    #[test]
    fn concat_string_0_args() {
        let s = concat_string!();
        assert_eq!(s, String::from(""));
    }

    #[test]
    fn concat_string_1_arg() {
        let s = concat_string!("foo");
        assert_eq!(s, String::from("foo"));
    }

    #[test]
    fn concat_string_str_string() {
        // Skipping formatting here because we want to test that the trailing comma works
        #[rustfmt::skip]
        let s = {
        concat_string!(
            "foo",
            String::from("bar"),
        )
        };
        assert_eq!(s, String::from("foobar"));
    }
}
