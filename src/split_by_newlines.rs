/// Splits a byte slice into line slices at newline boundaries.
/// Handles all common line ending formats: LF (\n), CR (\r), and CRLF (\r\n).
///
/// Returns slices that borrow from `data`, to avoid per-line allocation or copying
/// of the source. The line-ending bytes themselves are excluded from each returned
/// slice.
pub fn split_by_newlines(data: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut line_start = 0;
    let mut i = 0;

    while i < data.len() {
        match data[i] {
            // Handle CR (\r), possibly followed by LF (CRLF)
            b'\r' => {
                result.push(&data[line_start..i]);

                // Skip the LF part of CRLF
                if i + 1 < data.len() && data[i + 1] == b'\n' {
                    i += 1;
                }
                line_start = i + 1;
            }
            // Handle LF (\n)
            b'\n' => {
                result.push(&data[line_start..i]);
                line_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }

    // Push the last line if it doesn't end with a newline
    if line_start < data.len() {
        result.push(&data[line_start..]);
    }

    result
}
