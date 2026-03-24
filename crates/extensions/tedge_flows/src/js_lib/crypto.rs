use rquickjs::class::Trace;
use rquickjs::Ctx;
use rquickjs::JsLifetime;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct Crypto {}

pub fn init(ctx: &Ctx<'_>) {
    let crypto = Crypto {};
    let _ = ctx.globals().set("crypto", crypto);
}

#[rquickjs::methods]
impl Crypto {
    #[qjs(rename = "randomUUID")]
    fn random_uuid() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
