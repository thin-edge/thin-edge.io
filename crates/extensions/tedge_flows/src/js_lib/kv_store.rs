use crate::js_value::JsonValue;
use rquickjs::class::Trace;
use rquickjs::Ctx;
use rquickjs::IntoJs;
use rquickjs::JsLifetime;
use rquickjs::Object;
use rquickjs::Result;
use rquickjs::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default, JsLifetime)]
pub struct KVStore {
    data: Arc<Mutex<HashMap<String, BTreeMap<String, JsonValue>>>>,
}

pub const MAPPER_NAMESPACE: &str = "";

impl KVStore {
    pub fn init(&self, ctx: &Ctx<'_>) {
        self.store_as_userdata(ctx)
    }

    pub fn js_context<'js>(
        ctx: &Ctx<'js>,
        flow_name: &str,
        script_name: &str,
        config: &JsonValue,
    ) -> Result<Value<'js>> {
        let context = Object::new(ctx.clone())?;

        context.set("mapper", FlowStore::new(MAPPER_NAMESPACE))?;
        context.set("flow", FlowStore::new(flow_name))?;
        context.set("script", FlowStore::new(script_name))?;
        context.set("config", config)?;

        context.into_js(ctx)
    }

    pub fn get(&self, namespace: &str, key: &str) -> JsonValue {
        let data = self.data.lock().unwrap();
        match data.get(namespace) {
            None => JsonValue::Null,
            Some(map) => map.get(key).cloned().unwrap_or(JsonValue::Null),
        }
    }

    pub fn insert(&self, namespace: &str, key: &str, value: impl Into<JsonValue>) {
        match value.into() {
            JsonValue::Null => self.remove(namespace, key),
            value => {
                let mut data = self.data.lock().unwrap();
                let map = data.entry(namespace.to_string()).or_default();
                map.insert(key.to_owned(), value);
            }
        }
    }

    pub fn keys(&self, namespace: &str) -> Vec<String> {
        let data = self.data.lock().unwrap();
        match data.get(namespace) {
            None => vec![],
            Some(map) => map.keys().cloned().collect(),
        }
    }

    pub fn remove(&self, namespace: &str, key: &str) {
        let mut data = self.data.lock().unwrap();
        if let Some(map) = data.get_mut(namespace) {
            map.remove(key);
        }
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
struct FlowStore {
    namespace: String,
}

impl FlowStore {
    fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
        }
    }
}

#[rquickjs::methods]
impl<'js> FlowStore {
    fn get(&self, ctx: Ctx<'js>, key: String) -> Result<JsonValue> {
        let data = KVStore::get_from_userdata(&ctx);
        Ok(data.get(&self.namespace, &key))
    }

    fn set(&self, ctx: Ctx<'js>, key: String, value: JsonValue) {
        let data = KVStore::get_from_userdata(&ctx);
        data.insert(&self.namespace, &key, value)
    }

    fn remove(&self, ctx: Ctx<'js>, key: String) {
        let data = KVStore::get_from_userdata(&ctx);
        data.remove(&self.namespace, &key)
    }

    fn keys(&self, ctx: Ctx<'js>) -> Vec<String> {
        let data = KVStore::get_from_userdata(&ctx);
        data.keys(&self.namespace)
    }
}
