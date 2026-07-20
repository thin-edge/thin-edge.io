use proc_macro2::LineColumn;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;

/// Returns the start position of every identifier named `name` in `tokens`,
/// in token order
pub fn ident_positions(tokens: &TokenStream, name: &str) -> Vec<LineColumn> {
    let mut starts = Vec::new();
    collect_ident_positions(tokens.clone(), name, &mut starts);
    starts
}

fn collect_ident_positions(tokens: TokenStream, name: &str, starts: &mut Vec<LineColumn>) {
    for tt in tokens {
        match tt {
            TokenTree::Ident(i) if i == name => starts.push(i.span().start()),
            TokenTree::Group(g) => collect_ident_positions(g.stream(), name, starts),
            _ => {}
        }
    }
}

/// Queries a generated token stream for specific items
///
/// Parses the token stream into a `syn::File` and lets callers drill into
/// structs, fields, impl blocks, methods, and associated types without
/// asserting the full output.
pub struct TokenQuery {
    nodes: Vec<QueryNode>,
    description: String,
}

enum QueryNode {
    Item(syn::Item),
    Field(syn::Field),
    ImplItem(Box<syn::ImplItem>),
}

impl TokenQuery {
    pub fn new(tokens: &TokenStream) -> Self {
        let file: syn::File = syn::parse2(tokens.clone()).unwrap();
        Self {
            nodes: file.items.into_iter().map(QueryNode::Item).collect(),
            description: "generated token stream".into(),
        }
    }

    /// Keeps only struct definitions matching `name`
    pub fn find_struct(self, name: &str) -> Self {
        let nodes = self
            .nodes
            .into_iter()
            .filter(|node| matches!(node, QueryNode::Item(syn::Item::Struct(s)) if s.ident == name))
            .collect();
        Self {
            nodes,
            description: format!("struct `{name}`"),
        }
    }

    /// Selects named fields from the remaining struct definitions.
    pub fn find_field(self, name: &str) -> Self {
        let description = format!("field `{name}` in {}", self.description);
        let nodes = self
            .nodes
            .into_iter()
            .flat_map(|node| match node {
                QueryNode::Item(syn::Item::Struct(item)) => item
                    .fields
                    .into_iter()
                    .filter(|field| field.ident.as_ref().is_some_and(|ident| ident == name))
                    .map(QueryNode::Field)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        Self { nodes, description }
    }

    /// Keeps only trait impls matching `trait_name` and `self_ty`.
    pub fn find_impl(self, trait_name: &str, self_ty: &str) -> Self {
        let nodes = self
            .nodes
            .into_iter()
            .filter(|node| {
                let QueryNode::Item(syn::Item::Impl(item)) = node else {
                    return false;
                };
                item.trait_
                    .as_ref()
                    .is_some_and(|(_, path, _)| tokens_match(path, trait_name))
                    && tokens_match(&item.self_ty, self_ty)
            })
            .collect();
        Self {
            nodes,
            description: format!("impl `{trait_name}` for `{self_ty}`"),
        }
    }

    /// Selects named methods from the remaining impl blocks.
    pub fn find_method(self, name: &str) -> Self {
        let description = format!("method `{name}` in {}", self.description);
        let nodes = self
            .nodes
            .into_iter()
            .flat_map(|node| match node {
                QueryNode::Item(syn::Item::Impl(item)) => item
                    .items
                    .into_iter()
                    .filter(|item| matches!(item, syn::ImplItem::Fn(method) if method.sig.ident == name))
                    .map(Box::new)
                    .map(QueryNode::ImplItem)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        Self { nodes, description }
    }

    /// Selects named associated types from the remaining impl blocks.
    pub fn find_type(self, name: &str) -> Self {
        let description = format!("associated type `{name}` in {}", self.description);
        let nodes = self
            .nodes
            .into_iter()
            .flat_map(|node| match node {
                QueryNode::Item(syn::Item::Impl(item)) => item
                    .items
                    .into_iter()
                    .filter(|item| matches!(item, syn::ImplItem::Type(ty) if ty.ident == name))
                    .map(Box::new)
                    .map(QueryNode::ImplItem)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        Self { nodes, description }
    }

    /// Compares the single selected node with an expected syntax fragment.
    #[track_caller]
    pub fn assert_eq(mut self, expected: &TokenStream) {
        match self.nodes.len() {
            1 => {}
            0 => panic!(
                "TokenQuery expected exactly one match for {}, but found no matches",
                self.description
            ),
            count => panic!(
                "TokenQuery expected exactly one match for {}, but found {count} matches",
                self.description
            ),
        }
        let node = self.nodes.pop().unwrap();
        let (actual, expected) = match node {
            QueryNode::Item(item) => (quote::quote!(#item), expected.clone()),
            QueryNode::Field(field) => (
                quote::quote!(struct __TokenQuery { #field }),
                quote::quote!(struct __TokenQuery { #expected }),
            ),
            QueryNode::ImplItem(item) => (
                quote::quote!(impl __TokenQuery { #item }),
                quote::quote!(impl __TokenQuery { #expected }),
            ),
        };
        assert_tokens_eq(&actual, &expected);
    }
}

fn tokens_match(tokens: &impl quote::ToTokens, expected: &str) -> bool {
    quote::quote!(#tokens).to_string().replace(' ', "") == expected.replace(' ', "")
}

/// Compares two token streams by pretty-printing them through prettyplease
#[track_caller]
pub fn assert_tokens_eq(actual: &TokenStream, expected: &TokenStream) {
    let actual: syn::File = syn::parse2(actual.clone()).unwrap();
    let expected: syn::File = syn::parse2(expected.clone()).unwrap();
    pretty_assertions::assert_eq!(
        prettyplease::unparse(&actual),
        prettyplease::unparse(&expected),
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    #[should_panic(
        expected = "TokenQuery expected exactly one match for field `missing` in struct `Example`, but found no matches"
    )]
    fn query_error_names_a_missing_selection() {
        let generated = quote! {
            struct Example {
                present: String,
            }
        };

        TokenQuery::new(&generated)
            .find_struct("Example")
            .find_field("missing")
            .assert_eq(&quote!(missing: String,));
    }

    #[test]
    #[should_panic(
        expected = "TokenQuery expected exactly one match for struct `Example`, but found 2 matches"
    )]
    fn query_error_reports_multiple_matches() {
        let generated = quote! {
            struct Example;
            struct Example;
        };

        TokenQuery::new(&generated)
            .find_struct("Example")
            .assert_eq(&quote!(
                struct Example;
            ));
    }
}
