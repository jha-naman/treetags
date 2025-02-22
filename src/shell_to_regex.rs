pub fn shell_to_regex(s: &str) -> String {
    let mut regex = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '*' => regex.push_str(".*"),  // '*' becomes '.*'
            '?' => regex.push_str("."),   // '?' becomes '.'
            '.' => regex.push_str("\\."), // '.' becomes
            '[' => {
                regex.push('['); // '[' stays as it is
                while let Some(&next) = chars.peek() {
                    if next == ']' {
                        break;
                    }
                    regex.push(chars.next().unwrap());
                }
            }
            '\\' => {
                if let Some(&next) = chars.peek() {
                    regex.push('\\'); // escape the next character
                    regex.push(next);
                    chars.next(); // consume the escaped char
                }
            }
            _ => regex.push(c), // any other character remains unchanged
        }
    }

    regex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_star() {
        assert_eq!(shell_to_regex("foo*"), "foo.*");
    }

    #[test]
    fn test_convert_question_mark() {
        assert_eq!(shell_to_regex("bar?"), "bar.");
    }

    #[test]
    fn test_escape_dot() {
        assert_eq!(shell_to_regex("c\\.d"), "c\\.d");
    }

    #[test]
    fn test_convert_bracket() {
        assert_eq!(shell_to_regex("[abc][def]"), "[abc][def]");
    }

    #[test]
    fn test_escape_backslash() {
        assert_eq!(shell_to_regex("\\\\"), "\\\\");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(shell_to_regex(""), "");
    }

    #[test]
    fn test_complex_pattern() {
        assert_eq!(shell_to_regex("a*[b-e]*f\\.g?"), "a.*[b-e].*f\\.g.");
    }
}
