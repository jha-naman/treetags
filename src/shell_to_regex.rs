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
