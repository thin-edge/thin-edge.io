//! Pins the compile errors (and especially their spans) produced by invalid
//! `define_config!` schemas — errors should point at the offending part of
//! the schema, not at generated code

// rustc renders type paths differently across versions, so the expected
// output is pinned to the toolchain CI uses
#[rustversion::attr(before(1.99), ignore = "expected output is from rustc 1.99")]
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
