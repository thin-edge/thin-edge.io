use rand::RngExt;
use rquickjs::class::Trace;
use rquickjs::Ctx;
use rquickjs::IntoJs;
use rquickjs::JsLifetime;
use rquickjs::Object;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct Crypto {}

pub fn init(ctx: &Ctx<'_>) {
    let crypto = Crypto {};
    let _ = ctx.globals().set("crypto", crypto);
}

#[macro_export]
macro_rules! set_random_values {
    ( $ctx: ident, $array:ident : $ty:ident [ $len:expr ]) => {
        let mut rng = rand::rng();
        for i in 0..$len {
            let value = rng.random::<$ty>();
            if let Ok(js_value) = value.into_js(&$ctx) {
                let _ = $array.set(i as u32, js_value);
            }
        }
    };
}

#[rquickjs::methods]
impl<'js> Crypto {
    #[qjs(rename = "randomUUID")]
    fn random_uuid() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    #[qjs(rename = "getRandomValues")]
    fn random_u32(ctx: Ctx<'js>, object: Object<'js>) -> Object<'js> {
        if let Some(array) = object.as_typed_array::<u8>() {
            set_random_values!(ctx, object: u8[array.len()]);
        } else if let Some(array) = object.as_typed_array::<i8>() {
            set_random_values!(ctx, object: i8[array.len()]);
        } else if let Some(array) = object.as_typed_array::<u16>() {
            set_random_values!(ctx, object: u16[array.len()]);
        } else if let Some(array) = object.as_typed_array::<i16>() {
            set_random_values!(ctx, object: i16[array.len()]);
        } else if let Some(array) = object.as_typed_array::<u32>() {
            set_random_values!(ctx, object: u32[array.len()]);
        } else if let Some(array) = object.as_typed_array::<i32>() {
            set_random_values!(ctx, object: u32[array.len()]);
        } else if let Some(array) = object.as_typed_array::<u64>() {
            set_random_values!(ctx, object: u64[array.len()]);
        } else if let Some(array) = object.as_typed_array::<i64>() {
            set_random_values!(ctx, object: i64[array.len()]);
        } else if let Some(array) = object.as_typed_array::<f32>() {
            set_random_values!(ctx, object: f32[array.len()]);
        } else if let Some(array) = object.as_typed_array::<f64>() {
            set_random_values!(ctx, object: f64[array.len()]);
        }

        object
    }
}
