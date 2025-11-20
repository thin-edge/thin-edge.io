use crate::js_value::JsonValue;
use rquickjs::class::Trace;
use rquickjs::Class;
use rquickjs::Ctx;
use rquickjs::JsLifetime;
use rquickjs::Result;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default, JsLifetime)]
pub struct KVStore {
    data: Arc<Mutex<BTreeMap<String, JsonValue>>>,
}

impl KVStore {
    pub fn init(&self, ctx: &Ctx<'_>) {
        let globals = ctx.globals();
        let _ = Class::<FlowStore>::define(&globals);
        self.store_as_userdata(ctx)
    }

    pub fn get(&self, key: &str) -> JsonValue {
        let data = self.data.lock().unwrap();
        data.get(key).cloned().unwrap_or(JsonValue::Null)
    }

    pub fn insert(&self, key: impl Into<String>, value: impl Into<JsonValue>) {
        match value.into() {
            JsonValue::Null => self.remove(&key.into()),
            value => {
                let mut data = self.data.lock().unwrap();
                data.insert(key.into(), value);
            }
        }
    }

    pub fn remove(&self, key: &str) {
        let mut data = self.data.lock().unwrap();
        data.remove(key);
    }

    fn store_as_userdata(&self, ctx: &Ctx<'_>) {
        let _ = ctx.store_userdata(self.clone());
    }

    fn get_from_userdata(ctx: &Ctx<'_>) -> Self {
        match ctx.userdata::<Self>() {
            None => {
                let store = KVStore::default();
                store.store_as_userdata(ctx);
                store
            }
            Some(userdata) => userdata.deref().clone(),
        }
    }
}

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
pub struct FlowStore {}

#[rquickjs::methods]
impl<'js> FlowStore {
    #[qjs(constructor)]
    fn new(_ctx: Ctx<'js>) -> Result<FlowStore> {
        Ok(FlowStore {})
    }

    fn get(&self, ctx: Ctx<'js>, key: String) -> Result<JsonValue> {
        let data = KVStore::get_from_userdata(&ctx);
        Ok(data.get(&key))
    }
}
