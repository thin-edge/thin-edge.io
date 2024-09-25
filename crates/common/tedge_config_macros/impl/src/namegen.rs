use proc_macro2::Span;

/// Creates arbitrary [syn::Ident]s for binding variables within a macro
///
/// For instance, with "multi" fields, we have key enums such as
/// ```
/// pub enum ReadableKey {
///     C8yUrl(Option<String>),
///     MqttBindPort,
///     // ...
/// }
/// ```
///
/// and we wish to generate code, such as
///
/// ```
/// # pub enum ReadableKey {
/// #    C8yUrl(Option<String>),
/// #    MqttBindPort,
/// #    // ...
/// # }
/// #
/// fn do_something(k: &ReadableKey) {
///     match k {
///         ReadableKey::C8yUrl(key0) => todo!(),
///         ReadableKey::MqttBindPort => todo!(),
///     }
/// }
/// ```
///
/// we can use [SequentialIdGenerator] and [IdGenerator::next_id] to generate
/// `key0`, `key1`, etc.
///
/// If we wanted to discard the data
///
/// ```
/// # pub enum ReadableKey {
/// #     C8yUrl(Option<String>),
/// #     MqttBindPort,
/// #     // ...
/// # }
/// #
/// fn return_something(k: &ReadableKey) -> &'static str {
///     match k {
///         ReadableKey::C8yUrl(_) => "some independent result",
///         ReadableKey::MqttBindPort => "again, doesn't use the key apart from for matching",
///     }
/// }
/// ```
///
/// we can simply replace the usage of [SequentialIdGenerator] with [UnderscoreIdGenerator]
pub trait IdGenerator: Default {
    fn next_id(&mut self, span: Span) -> syn::Ident;
}

#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    pub count: u32,
}

impl SequentialIdGenerator {
    pub fn replay(&self, span: Span) -> syn::Ident {
        let i = self.count - 1;
        syn::Ident::new(&format!("key{i}"), span)
    }
}

#[derive(Debug, Default)]
pub struct UnderscoreIdGenerator;

impl IdGenerator for SequentialIdGenerator {
    fn next_id(&mut self, span: Span) -> syn::Ident {
        let i = self.count;
        self.count += 1;
        syn::Ident::new(&format!("key{i}"), span)
    }
}

impl Iterator for SequentialIdGenerator {
    type Item = syn::Ident;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_id(Span::call_site()))
    }
}

impl IdGenerator for UnderscoreIdGenerator {
    fn next_id(&mut self, span: Span) -> syn::Ident {
        syn::Ident::new("_", span)
    }
}

impl Iterator for UnderscoreIdGenerator {
    type Item = syn::Ident;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_id(Span::call_site()))
    }
}
