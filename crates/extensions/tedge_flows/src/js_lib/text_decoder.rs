use rquickjs::class::Trace;
use rquickjs::prelude::Opt;
use rquickjs::Class;
use rquickjs::Ctx;
use rquickjs::Exception;
use rquickjs::JsLifetime;
use rquickjs::Object;
use rquickjs::Result;
use rquickjs::TypedArray;

#[derive(Clone, Default, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct TextDecoder {
    fatal: bool,
    ignore_bom: bool,
}

pub fn init(ctx: &Ctx<'_>) {
    let globals = ctx.globals();
    let _ = Class::<TextDecoder>::define(&globals);
}

#[rquickjs::methods]
impl<'js> TextDecoder {
    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>, label: Opt<String>, options: Opt<Object<'js>>) -> Result<TextDecoder> {
        if let Some(label) = label.into_inner() {
            if label != "utf-8" && label != "utf8" {
                return Err(Exception::throw_message(
                    &ctx,
                    "TextDecoder only supports utf-8",
                ));
            }
        }
        let decoder = options.into_inner().map(|options| {
            let fatal = options.get("fatal").ok().flatten().unwrap_or(false);
            let ignore_bom = options.get("ignoreBOM").ok().flatten().unwrap_or(false);
            TextDecoder { fatal, ignore_bom }
        });

        Ok(decoder.unwrap_or_default())
    }

    #[qjs(get)]
    fn encoding(&self) -> &str {
        "utf-8"
    }

    #[qjs(get)]
    fn fatal(&self) -> bool {
        self.fatal
    }

    #[qjs(get, rename = "ignoreBOM")]
    fn ignore_bom(&self) -> bool {
        self.ignore_bom
    }

    pub fn decode(&self, ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> Result<String> {
        let mut bytes = bytes
            .as_bytes()
            .ok_or(Exception::throw_message(&ctx, "ArrayBuffer is detached"))?;

        if !self.ignore_bom && bytes.get(..3) == Some(&[0xEF, 0xBB, 0xBF]) {
            bytes = &bytes[3..];
        }

        let text = if self.fatal {
            std::str::from_utf8(bytes)
                .map_err(|err| Exception::throw_message(&ctx, &err.to_string()))?
                .to_string()
        } else {
            String::from_utf8_lossy(bytes).to_string()
        };

        Ok(text)
    }
}
