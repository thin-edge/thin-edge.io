use serde::Serialize;
use tedge_derive::serde_other;

#[serde_other]
#[derive(Serialize, Debug, Clone, Default)]
struct Foo {}

#[test]
fn new_field_is_added() {
    let bar = Foo::default();
    assert!(bar.other.is_empty());
}
