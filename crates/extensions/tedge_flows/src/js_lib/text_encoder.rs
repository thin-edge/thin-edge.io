use rquickjs::class::Trace;
use rquickjs::Class;
use rquickjs::Ctx;
use rquickjs::JsLifetime;
use rquickjs::Result;
use rquickjs::TypedArray;
use rquickjs::Value;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct TextEncoder {}

pub fn init(ctx: &Ctx<'_>) {
    let globals = ctx.globals();
    let _ = Class::<TextEncoder>::define(&globals);
}

#[rquickjs::methods]
impl<'js> TextEncoder {
    #[qjs(constructor)]
    fn new() -> TextEncoder {
        TextEncoder {}
    }

    #[qjs(get)]
    fn encoding(&self) -> &str {
        "utf-8"
    }

    pub fn encode(&self, ctx: Ctx<'js>, text: Value<'js>) -> Result<TypedArray<'js, u8>> {
        let string = match text.as_string() {
            None => {
                if let Some(object) = text.as_object() {
                    if let Some(bytes) = object.as_typed_array::<u8>() {
                        return Ok(bytes.clone());
                    }
                }
                "".to_string()
            }
            Some(js_string) => js_string.to_string()?,
        };
        TypedArray::new(ctx.clone(), string.as_bytes())
    }
}
