//! Pins the compile errors (and especially their spans) produced by invalid
//! `define_config!` schemas — errors should point at the offending part of
//! the schema, not at generated code

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
