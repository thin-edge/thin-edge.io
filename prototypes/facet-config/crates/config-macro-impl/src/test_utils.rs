use proc_macro2::LineColumn;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;

/// Returns the start position of every identifier named `name` in `tokens`,
/// in token order
pub fn ident_starts(tokens: &TokenStream, name: &str) -> Vec<LineColumn> {
    let mut starts = Vec::new();
    collect_ident_starts(tokens.clone(), name, &mut starts);
    starts
}

fn collect_ident_starts(tokens: TokenStream, name: &str, starts: &mut Vec<LineColumn>) {
    for tt in tokens {
        match tt {
            TokenTree::Ident(i) if i == name => starts.push(i.span().start()),
            TokenTree::Group(g) => collect_ident_starts(g.stream(), name, starts),
            _ => {}
        }
    }
}

/// Returns the position of the first occurrence of `needle` in `src`, using
/// the same convention as [`LineColumn`] (1-based line, 0-based column)
pub fn position_of(src: &str, needle: &str) -> LineColumn {
    let offset = src
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in source"));
    let line = src[..offset].matches('\n').count() + 1;
    let column = src[..offset]
        .rfind('\n')
        .map_or(offset, |nl| offset - nl - 1);
    LineColumn { line, column }
}
