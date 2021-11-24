/// Returns a substring of `s` starting at `line` and `column`. At most `max_chars` are returned.
pub(crate) fn excerpt(s: &str, line: usize, column: usize, max_chars: usize) -> String {
    let mut current_line = 1;
    let mut chars = s.chars();

    while current_line < line {
        match chars.next() {
            Some(ch) => {
                if ch == '\n' {
                    current_line += 1;
                }
            }
            None => {
                break;
            }
        }
    }

    // Seek forward. We cannot use `skip`, as we then can no longer call `as_str`.
    let mut current_column = 1;
    while current_column < column {
        match chars.next() {
            Some(_) => {
                current_column += 1;
            }
            None => break,
        }
    }

    // Don't use byte slicing for UTF-8 strings, e.g. `[..80]`, as this might panic in case of a
    // wide-character at this position.
    let mut excerpt = String::with_capacity(max_chars);
    for _i in 1..=max_chars {
        match chars.next() {
            Some(ch) => excerpt.push(ch),
            None => break,
        }
    }

    excerpt
}

#[test]
fn excerpt_returns_string_starting_from_line_and_column() {
    assert_eq!(
        "ne 2\nline 3\n",
        excerpt("line 1\nline 2\nline 3\n", 2, 3, 80)
    );
    assert_eq!("n", excerpt("line 1\nline 2\nline 3\n", 2, 3, 1));
}
