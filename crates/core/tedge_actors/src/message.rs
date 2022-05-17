use std::fmt::Debug;

/// A message exchanged between two actors
pub trait Message: 'static + Clone + Debug + Send + Sync {}

/// Strings can be used as Message
impl Message for String {}

/// An actor can have no input or no output messages
#[derive(Clone, Debug)]
pub enum NoMessage {}
impl Message for NoMessage {}

/// A macro to define a enum type grouping several message types
///
/// `message_type!(Input[Msg1,Msg2]);` expends to:
///
/// ```no_run
/// # use tedge_actors::Message;
/// # #[derive(Clone, Debug)]
/// # struct Msg1 {}
/// # #[derive(Clone, Debug)]
/// # struct Msg2 {}
///
/// #[derive(Clone, Debug)]
/// enum Input {
///     Msg1(Msg1),
///     Msg2(Msg2),
/// }
/// impl Message for Input {}
/// impl Into<Input> for Msg1 {
///     fn into(self) -> Input {
///        Input::Msg1(self)
///     }
/// }
/// impl Into<Input> for Msg2 {
///     fn into(self) -> Input {
///        Input::Msg2(self)
///     }
/// }
/// ```
#[macro_export]
macro_rules! message_type {
    ( $t:ident [ $( $x:ident ),* ] ) => {
        #[derive(Clone, Debug)]
        enum $t {
            $(
                $x($x),
            )*
        }
        impl Message for Input {}
        $(
            impl Into<$t> for $x {
                fn into(self) -> $t {
                    $t::$x(self)
                }
            }
        )*
    };
}
