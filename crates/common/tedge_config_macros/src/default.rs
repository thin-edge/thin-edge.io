use camino::Utf8Path;

/// An abstraction over the possible default functions for tedge config values
///
/// Some configuration defaults are relative to the config location, and
/// this trait allows us to pass that in, or the DTO, both, or neither!
pub trait TEdgeConfigDefault<T, Args> {
    type Output;
    fn call(self, data: &T, location: &TEdgeConfigLocation) -> Self::Output;
}

/// A dummy tedge config location
///
/// In practice, we use `TEdgeConfigLocation` from the `tedge_config` crate.
/// This is defined simply to allow us to create examples in this crate.
#[derive(Default)]
pub struct TEdgeConfigLocation;

impl TEdgeConfigLocation {
    pub fn tedge_config_root_path(&self) -> &Utf8Path {
        "/etc/tedge".into()
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, ()> for F
where
    F: FnOnce() -> Out + Clone,
{
    type Output = Out;
    fn call(self, _: &T, _: &TEdgeConfigLocation) -> Self::Output {
        (self)()
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, &T> for F
where
    F: FnOnce(&T) -> Out + Clone,
{
    type Output = Out;
    fn call(self, data: &T, _location: &TEdgeConfigLocation) -> Self::Output {
        (self)(data)
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, (&TEdgeConfigLocation,)> for F
where
    F: FnOnce(&TEdgeConfigLocation) -> Out + Clone,
{
    type Output = Out;
    fn call(self, _data: &T, location: &TEdgeConfigLocation) -> Self::Output {
        (self)(location)
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, (&T, &TEdgeConfigLocation)> for F
where
    F: FnOnce(&T, &TEdgeConfigLocation) -> Out + Clone,
{
    type Output = Out;
    fn call(self, data: &T, location: &TEdgeConfigLocation) -> Self::Output {
        (self)(data, location)
    }
}
