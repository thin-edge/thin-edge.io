/// A macro to define a enum type grouping several message types
///
/// `message_type!(Msg[Msg1,Msg2]);` expends to:
///
/// ```no_run
/// # use tedge_actors::Message;
/// # #[derive(Clone, Debug)]
/// # struct Msg1 {}
/// # #[derive(Clone, Debug)]
/// # struct Msg2 {}
///
/// #[derive(Clone, Debug)]
/// enum Msg {
///     Msg1(Msg1),
///     Msg2(Msg2),
/// }
/// impl Message for Msg {}
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
macro_rules! message_type {
    ( $t:ident [ $( $x:ident ),* ] ) => {
        #[derive(Clone, Debug)]
        pub enum $t {
            $(
                $x($x),
            )*
        }
        impl Message for $t {}
        $(
            impl From<$x> for $t {
                fn from(m: $x) -> $t {
                    $t::$x(m)
                }
            }
        )*
    };
}
