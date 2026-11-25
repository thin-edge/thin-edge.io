//! Pins the compile errors (and especially their spans) produced by invalid
//! `define_config!` schemas — errors should point at the offending part of
//! the schema, not at generated code

// rustc's error output changes between versions, and the expected output
// matches Rust 1.100, so older compilers would report false mismatches.
// This must come before #[test], which only sees attributes already expanded
#[rustversion::attr(
    before(1.100),
    ignore = "the expected output matches the errors of Rust 1.100 and later"
)]
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
