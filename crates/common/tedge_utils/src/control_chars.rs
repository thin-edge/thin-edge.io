//! Removal of the control characters from a text, e.g. a command output,
//! along with the terminal escape sequences they introduce.

use std::iter::Peekable;
use std::ops::RangeInclusive;
use std::str::Chars;

// The ECMA-48 characters and byte ranges used to recognize the terminal escape sequences
const ESC: char = '\x1b';
const BEL: char = '\x07';
const ST: char = '\u{9c}';
const INTERMEDIATE: RangeInclusive<char> = '\x20'..='\x2f';
const FINAL: RangeInclusive<char> = '\x30'..='\x7e';
const CSI_PARAMETER_OR_INTERMEDIATE: RangeInclusive<char> = '\x20'..='\x3f';
const CSI_FINAL: RangeInclusive<char> = '\x40'..='\x7e';

/// Removes the control characters and the terminal escape sequences from a text.
///
/// - Removes all control characters except for `\n`, `\t`, `\r`.
/// - Removes the terminal escape sequences, e.g. colors, introduced by an `ESC` control character,
///   so that no meaningless leftovers, e.g. `[31m`, are kept.
pub fn strip_control_chars(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ESC {
            skip_escape_sequence(&mut chars);
        } else if c == '\r' || c == '\n' || c == '\t' || !c.is_control() {
            output.push(c);
        }
    }
    output
}

/// Skips the escape sequence following an `ESC` character, as defined by ECMA-48.
///
/// Consumes the longest prefix matching an escape sequence:
/// a sequence missing its final byte is dropped up to where it stops matching.
fn skip_escape_sequence(chars: &mut Peekable<Chars>) {
    match chars.peek() {
        // Control Sequence, e.g. the colors: parameter and intermediate bytes, then a final byte
        Some('[') => {
            chars.next();
            while chars
                .next_if(|c| CSI_PARAMETER_OR_INTERMEDIATE.contains(c))
                .is_some()
            {}
            chars.next_if(|c| CSI_FINAL.contains(c));
        }
        // Control String, e.g. a hyperlink or a window title: terminated by `BEL`, `ST` or `ESC \`.
        // An unterminated string is not skipped past the end of the line,
        // so a stray escape character doesn't remove the following lines.
        Some(']' | 'P' | 'X' | '^' | '_') => {
            chars.next();
            // The loop stops before an `ESC` and leaves it to the caller:
            // `ESC \` ends the string, any other `ESC` starts a new sequence
            while let Some(c) = chars.next_if(|c| *c != '\n' && *c != ESC) {
                if c == BEL || c == ST {
                    break;
                }
            }
        }
        // Other escape sequences: intermediate bytes, then a final byte
        _ => {
            while chars.next_if(|c| INTERMEDIATE.contains(c)).is_some() {}
            chars.next_if(|c| FINAL.contains(c));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn selected_control_chars_remain() {
        let stripped = strip_control_chars("\0\t\n\r");
        assert_eq!(stripped, "\t\n\r");
    }

    #[test]
    fn control_chars_are_removed() {
        let input: String = ('\x00'..='\x7f').collect();
        let stripped = strip_control_chars(&input);
        assert_eq!(
            stripped,
            "\t\n\r !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~"
        );
    }

    #[rstest]
    #[case::colors("\x1b[31mhello\x1b[0m\n", "hello\n")]
    #[case::two_character_escape_sequence("a\x1bb", "a")]
    #[case::control_sequence_with_parameters("\x1b[1;38;5;196mbold\x1b[m", "bold")]
    #[case::control_sequence_without_final_byte("\x1b[12\nnext", "\nnext")]
    #[case::hyperlink("\x1b]8;;https://example.com\x07link\x1b]8;;\x1b\\", "link")]
    #[case::control_string_terminated_by_st("\x1b]0;title\u{9c}visible", "visible")]
    #[case::escape_sequence_inside_control_string("\x1b]0;t\x1b[31mred", "red")]
    #[case::unterminated_control_string("\x1b]0;title\nnext line", "\nnext line")]
    #[case::trailing_escape("trailing\x1b", "trailing")]
    #[case::non_ascii_printable_chars("✓ ├╴╰─▶", "✓ ├╴╰─▶")]
    #[case::non_ascii_and_control_chars("こんにちは\"\n\0こんにちは", "こんにちは\"\nこんにちは")]
    fn control_chars_and_escape_sequences_are_removed(
        #[case] input: &str,
        #[case] expected_output: &str,
    ) {
        let stripped = strip_control_chars(input);
        assert_eq!(stripped, expected_output);
    }

    #[test]
    fn many_nested_escape_sequences_do_not_overflow_the_stack() {
        let input = format!("{}\nvisible", "\x1b]".repeat(1_000_000));
        let stripped = strip_control_chars(&input);
        assert_eq!(stripped, "\nvisible");
    }
}
