/// A macro to define an enum type grouping several message types
///
/// `fan_in_message_type!(Msg[Msg1,Msg2] : Clone, Debug);` expends to:
///
/// ```no_run
/// # use tedge_actors::Message;
/// # #[derive(Clone, Debug)]
/// # pub struct Msg1 {}
/// # #[derive(Clone, Debug)]
/// # pub struct Msg2 {}
///
/// #[derive(Clone, Debug)]
/// pub enum Msg {
///     Msg1(Msg1),
///     Msg2(Msg2),
/// }
/// impl From<Msg1> for Msg {
///     fn from(m: Msg1) -> Msg {
///        Msg::Msg1(m)
///     }
/// }
/// impl From<Msg2> for Msg {
///     fn from(m: Msg2) -> Msg {
///        Msg::Msg2(m)
///     }
/// }
/// ```
#[macro_export]
macro_rules! fan_in_message_type {
    ( $t:ident [ $( $x:ident ),* ] : $( $d:ident ),*) => {
        #[derive(
            $(
                $d,
            )*
        )]
        pub enum $t {
            $(
                $x($x),
            )*
        }
        $(
            impl From<$x> for $t {
                fn from(m: $x) -> $t {
                    $t::$x(m)
                }
            }
        )*
    };
}