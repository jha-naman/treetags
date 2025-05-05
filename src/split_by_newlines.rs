/// Splits a Vec<u8> into multiple vectors at newline boundaries.
/// Handles all common line ending formats: LF (\n), CR (\r), and CRLF (\r\n).
pub fn split_by_newlines(data: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let mut current_line = Vec::new();
    let mut i = 0;

    while i < data.len() {
        match data[i] {
            // Handle CR (\r)
            b'\r' => {
                // Push the current line (without the CR)
                result.push(current_line);
                current_line = Vec::new();

                // Check if the next byte is LF (for CRLF)
                if i + 1 < data.len() && data[i + 1] == b'\n' {
                    i += 1; // Skip the LF part of CRLF
                }
            }
            // Handle LF (\n)
            b'\n' => {
                // Push the current line (without the LF)
                result.push(current_line);
                current_line = Vec::new();
            }
            // Regular byte - add to current line
            _ => {
                current_line.push(data[i]);
            }
        }
        i += 1;
    }

    // Don't forget to push the last line if it doesn't end with a newline
    if !current_line.is_empty() {
        result.push(current_line);
    }

    result
}
