use serde::Serialize;
use tedge_derive::add_other_field;

#[deserialize_rest]
#[derive(Serialize, Debug, Clone, Default)]
struct Foo {}

#[test]
fn new_field_is_added() {
    let bar = Foo::default();
    assert!(bar.other.is_empty());
}
