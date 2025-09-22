use rquickjs::class::Trace;
use rquickjs::Class;
use rquickjs::Ctx;
use rquickjs::Exception;
use rquickjs::JsLifetime;
use rquickjs::Object;
use rquickjs::Result;
use rquickjs::TypedArray;

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

    pub fn encode(
        &self,
        ctx: Ctx<'js>,
        text: rquickjs::String<'js>,
    ) -> Result<TypedArray<'js, u8>> {
        let string = text.to_string()?;
        TypedArray::new(ctx.clone(), string.as_bytes())
    }

    #[qjs(rename = "encodeInto")]
    pub fn encode_into(
        &self,
        ctx: Ctx<'js>,
        text: rquickjs::String<'js>,
        array: TypedArray<'js, u8>,
    ) -> Result<Object<'js>> {
        let string = text.to_string()?;
        let offset: usize = array.get("byteOffset").unwrap_or_default();
        let buffer = array
            .arraybuffer()?
            .as_raw()
            .ok_or(Exception::throw_message(&ctx, "ArrayBuffer is detached"))?;

        let mut read = 0;
        let mut written = 0;
        let max_len = buffer.len - offset;
        for char in string.chars() {
            let len = char.len_utf8();
            if written + len > max_len {
                break;
            }
            read += char.len_utf16();
            written += len;
        }

        let bytes = &string.as_bytes()[..written];
        unsafe {
            let buffer_ptr =
                std::slice::from_raw_parts_mut(buffer.ptr.as_ptr().add(offset), written);
            buffer_ptr.copy_from_slice(bytes);
        }

        let obj = Object::new(ctx)?;
        obj.set("read", read)?;
        obj.set("written", written)?;
        Ok(obj)
    }
}
