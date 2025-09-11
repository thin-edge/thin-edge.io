use rquickjs::class::Trace;
use rquickjs::Class;
use rquickjs::Ctx;
use rquickjs::Exception;
use rquickjs::JsLifetime;
use rquickjs::Result;
use rquickjs::TypedArray;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct TextDecoder {}

pub fn init(ctx: &Ctx<'_>) {
    let globals = ctx.globals();
    let _ = Class::<TextDecoder>::define(&globals);
}

#[rquickjs::methods]
impl<'js> TextDecoder {
    #[qjs(constructor)]
    fn new() -> TextDecoder {
        TextDecoder {}
    }

    #[qjs(get)]
    fn encoding(&self) -> &str {
        "utf-8"
    }

    pub fn decode(&self, ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> Result<String> {
        let bytes = bytes
            .as_bytes()
            .ok_or(Exception::throw_message(&ctx, "ArrayBuffer is detached"))?;
        let text = std::str::from_utf8(bytes)
            .map_err(|err| Exception::throw_message(&ctx, &err.to_string()))?;

        Ok(text.to_owned())
    }
}
