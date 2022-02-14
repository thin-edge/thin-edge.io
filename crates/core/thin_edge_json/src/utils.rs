/// Returns a substring of `s` starting at `line` and `column`. At most `max_chars` are returned.
pub(crate) fn excerpt(s: &str, line: usize, column: usize, max_chars: usize) -> String {
    s.lines() // omits the newlines
        .skip(line - 1)
        .map(|line| {
            line.chars().chain(std::iter::once('\n')) // adds the newlines again
        })
        .flatten()
        .skip(column - 1)
        .take(max_chars)
        .collect()
}

#[test]
fn excerpt_returns_string_starting_from_line_and_column() {
    assert_eq!(
        "ne 2\nline 3\n",
        excerpt("line 1\nline 2\nline 3\n", 2, 3, 80)
    );
    assert_eq!("n", excerpt("line 1\nline 2\nline 3\n", 2, 3, 1));
}

#[test]
fn excerpt_returns_string_starting_from_line_and_column_but_limits_output() {
    let expected = "ne 2\nli";
    let result = excerpt("line 1\nline 2\nline 3\n", 2, 3, 7);
    assert_eq!(expected, result);
}

#[test]
fn excerpt_counts_newline_as_one_char_from_beginning() {
    let expected = "\n\n\n";
    let result = excerpt("\n\n\n\n", 1, 1, 3);
    assert_eq!(expected, result);
}

#[test]
fn excerpt_counts_newline_as_one_char_in_between_lines() {
    let expected = "\n\n";
    let result = excerpt("\n\n\n\n", 2, 1, 2);
    assert_eq!(expected, result);
}
