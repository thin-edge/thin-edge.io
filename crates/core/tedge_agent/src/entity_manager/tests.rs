use crate::entity_manager::server::EntityStoreResponse;
use crate::entity_manager::tests::model::Action;
use crate::entity_manager::tests::model::Action::AddDevice;
use crate::entity_manager::tests::model::Action::AddService;
use crate::entity_manager::tests::model::Command;
use crate::entity_manager::tests::model::Commands;
use crate::entity_manager::tests::model::Protocol::HTTP;
use crate::entity_manager::tests::model::Protocol::MQTT;
use proptest::proptest;
use serde_json::json;
use std::collections::HashSet;
use tedge_actors::Server;
use tedge_api::entity::EntityMetadata;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;

#[tokio::test]
async fn new_entity_store() {
    let (mut entity_store, _mqtt_output) = entity::server("device-under-test");

    assert_eq!(
        entity::get(&mut entity_store, "device/main//").await,
        Some(EntityMetadata::main_device())
    )
}

#[tokio::test]
async fn removing_an_unknown_child_using_mqtt() {
    let registrations = vec![
        // tedge mqtt pub -r te/device/a// ''
        Command {
            protocol: MQTT,
            action: Action::RemDevice {
                topic: "a".to_string(),
            },
        },
    ];
    check_registrations(Commands(registrations)).await
}

#[tokio::test]
async fn removing_a_child_using_mqtt() {
    let registrations = vec![
        // tedge http post /tedge/v1/entities '{"@parent":"device/main//","@topic-id":"device/a//","@type":"child-device"}'
        Command {
            protocol: HTTP,
            action: Action::AddDevice {
                topic: "a".to_string(),
                props: vec![],
            },
        },
        // tedge mqtt pub -r te/device/a// ''
        Command {
            protocol: MQTT,
            action: Action::RemDevice {
                topic: "a".to_string(),
            },
        },
    ];
    check_registrations(Commands(registrations)).await
}

#[tokio::test]
async fn patched_twin_fragments_published_to_mqtt() {
    let (mut entity_store, mut mqtt_box) = entity::server("device-under-test");
    entity::set_twin_fragments(
        &mut entity_store,
        EntityTopicId::default_main_device(),
        json!({"x": 9, "y": true, "z": "foo"})
            .as_object()
            .unwrap()
            .clone(),
    )
    .await
    .unwrap();
    assert_received_contains_str(&mut mqtt_box, [("te/device/main///twin/x", "9")]).await;
    assert_received_contains_str(&mut mqtt_box, [("te/device/main///twin/y", "true")]).await;
    assert_received_contains_str(&mut mqtt_box, [("te/device/main///twin/z", "foo")]).await;
}

proptest! {
    //#![proptest_config(proptest::prelude::ProptestConfig::with_cases(1000))]
    #[test]
    fn it_works_for_any_registration_order(registrations in model::walk(10)) {
        tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(check_registrations(registrations))
    }
}

async fn check_registrations(registrations: Commands) {
    let (mut entity_store, _mqtt_output) = entity::server("device-under-test");
    let mut state = model::State::new();

    for Command { protocol, action } in registrations.0 {
        let expected_updates = state.apply(protocol, action.clone());
        let actual_updates = match entity_store.handle((protocol, action).into()).await {
            EntityStoreResponse::Create(Ok(registered_entities)) => registered_entities
                .iter()
                .map(|registered_entity| registered_entity.reg_message.topic_id.clone())
                .collect(),
            EntityStoreResponse::Delete(actual_updates) => actual_updates
                .into_iter()
                .map(|meta| meta.topic_id)
                .collect(),
            _ => HashSet::new(),
        };
        assert_eq!(actual_updates, expected_updates);
    }

    let mut registered_topics: Vec<_> = entity_store.entity_topic_ids().collect();
    registered_topics.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));

    let mut expected_topics: Vec<_> = state.entity_topic_ids().collect();
    expected_topics.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));

    assert_eq!(registered_topics, expected_topics);

    for topic in registered_topics {
        let registered = entity_store.get(topic).unwrap();
        let (entity_type, parent, _) = state.get(topic).unwrap();
        assert_eq!(&registered.r#type, entity_type);
        assert_eq!(registered.parent.as_ref(), parent.as_ref());
    }
}

proptest! {
    #[test]
    fn it_works_from_user_pov(registrations in model::walk(10)) {
        tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(check_registrations_from_user_pov(registrations))
    }
}

async fn check_registrations_from_user_pov(registrations: Commands) {
    let (mut entity_store, _mqtt_output) = entity::server("device-under-test");

    // Trigger all operations over HTTP to avoid pending entities (which are not visible to the user)
    for action in registrations.0.into_iter().map(|c| c.action) {
        let parent_id = action.parent_topic_id();
        let topic_id = action.topic_id();
        let entity_type = action.target_type();

        match &action {
            AddDevice { .. } | AddService { .. } => {
                let previous = entity_store.get(&topic_id).cloned();
                if previous.is_none()
                    && (parent_id.is_none()
                        || entity_store.get(parent_id.as_ref().unwrap()).is_some())
                {
                    // If not registered and with a parent that is registered
                    // then the new entity should be registered
                    assert!(matches!(
                        entity_store.handle((HTTP, action).into()).await,
                        EntityStoreResponse::Create(Ok(_))
                    ));
                    let registered = entity_store.get(&topic_id).unwrap();
                    assert_eq!(registered.parent, parent_id);
                    assert_eq!(registered.r#type, entity_type);
                } else {
                    // If already registered and with a parent that is not registered
                    // then the registration should be rejected
                    // and the previous entity be unchanged, if any
                    assert!(matches!(
                        entity_store.handle((HTTP, action).into()).await,
                        EntityStoreResponse::Create(Err(_))
                    ));
                    assert_eq!(previous.as_ref(), entity_store.get(&topic_id));
                }
            }

            Action::RemDevice { .. } | Action::RemService { .. } => {
                entity_store.handle((HTTP, action).into()).await;
                assert!(entity_store.get(&topic_id).is_none());
            }
        }
    }
}

mod entity {
    use crate::entity_manager::server::EntityStoreRequest;
    use crate::entity_manager::server::EntityStoreResponse;
    use crate::entity_manager::server::EntityStoreServer;
    use serde_json::Map;
    use serde_json::Value;
    use std::str::FromStr;
    use tedge_actors::Builder;
    use tedge_actors::NoMessage;
    use tedge_actors::Server;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::entity::EntityMetadata;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::EntityStore;
    use tedge_mqtt_ext::MqttMessage;
    use tempfile::TempDir;

    pub async fn get(
        entity_store: &mut EntityStoreServer,
        topic_id: &str,
    ) -> Option<EntityMetadata> {
        let topic_id = EntityTopicId::from_str(topic_id).unwrap();
        if let EntityStoreResponse::Get(entity) =
            entity_store.handle(EntityStoreRequest::Get(topic_id)).await
        {
            return entity;
        };
        None
    }

    pub async fn set_twin_fragments(
        entity_store: &mut EntityStoreServer,
        topic_id: EntityTopicId,
        fragments: Map<String, Value>,
    ) -> Result<(), anyhow::Error> {
        if let EntityStoreResponse::SetTwinFragments(result) = entity_store
            .handle(EntityStoreRequest::SetTwinFragments(topic_id, fragments))
            .await
        {
            return result.map_err(Into::into);
        };
        anyhow::bail!("Unexpected response");
    }

    pub fn server(
        device_id: &str,
    ) -> (EntityStoreServer, SimpleMessageBox<MqttMessage, NoMessage>) {
        let mqtt_schema = MqttSchema::default();
        let main_device = EntityRegistrationMessage::main_device(Some(device_id.to_string()));
        let telemetry_cache_size = 0;
        let log_dir = TempDir::new().unwrap();
        let clean_start = true;
        let entity_auto_register = true;
        let entity_store = EntityStore::with_main_device(
            mqtt_schema.clone(),
            main_device,
            telemetry_cache_size,
            log_dir,
            clean_start,
        )
        .unwrap();

        let mut mqtt_actor = SimpleMessageBoxBuilder::new("MQTT", 64);
        let server = EntityStoreServer::new(
            entity_store,
            mqtt_schema,
            &mut mqtt_actor,
            entity_auto_register,
        );

        let mqtt_output = mqtt_actor.build();
        (server, mqtt_output)
    }
}

mod model {
    use crate::entity_manager::server::EntityStoreRequest;
    use proptest::prelude::*;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use tedge_api::entity::EntityType;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::Channel;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::MqttMessage;

    #[derive(Clone)]
    pub struct Commands(pub Vec<Command>);

    impl Debug for Commands {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            // mimicking a sequence of cli commands, with no extra quotes
            // e.g:
            //     tedge mqtt pub -r device/main/service/a '{"@parent":"device/main//","@type":"service","x":"9"}' \
            //  && tedge http post /tedge/v1/entities '{"@parent":"device/main//","@topic-id":"device/c//","@type":"child-device","z":"5"}'
            let mut sep = if f.alternate() {
                "\n    " // On test unit output, print each command on a new line
            } else {
                "" // On proptest log, print all the commands on a single line
            };

            for command in &self.0 {
                f.write_str(sep)?;
                if f.alternate() {
                    // On test unit output, print each command on a new line (using Rust notation)
                    sep = "\n    ";
                    f.write_str(format!("// {command}{sep}").as_str())?;
                    f.write_str(&ron::to_string(&command).unwrap())?;
                } else {
                    // On proptest log, print all the commands on a single line (using shell commands)
                    sep = " && ";
                    f.write_str(format!("{command}").as_str())?;
                }
            }
            f.write_str("\n")?;
            Ok(())
        }
    }

    #[derive(Clone, serde::Serialize)]
    pub struct Command {
        pub protocol: Protocol,
        pub action: Action,
    }

    impl Debug for Command {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            // Print the command line with no extra quotes
            f.write_str(format!("{self}").as_str())
        }
    }

    impl Display for Command {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            let topic = self.action.topic_id().to_string();

            let cmd = match self.action {
                Action::AddDevice { .. } | Action::AddService { .. } => {
                    let payload = self.action.payload();
                    match self.protocol {
                        Protocol::HTTP => {
                            let mut payload = payload;
                            payload.insert("@topic-id".to_string(), topic.into());
                            let payload = serde_json::Value::Object(payload).to_string();
                            format!("tedge http post /tedge/v1/entities '{payload}'")
                        }
                        Protocol::MQTT => {
                            let payload = serde_json::Value::Object(payload).to_string();
                            format!("tedge mqtt pub -r te/{topic} '{payload}'")
                        }
                    }
                }
                Action::RemDevice { .. } | Action::RemService { .. } => match self.protocol {
                    Protocol::HTTP => {
                        format!("tedge http delete /tedge/v1/entities/{topic}")
                    }
                    Protocol::MQTT => {
                        format!("tedge mqtt pub -r te/{topic} ''")
                    }
                },
            };

            // Print the command line with no extra quotes
            f.write_str(cmd.as_str())
        }
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Serialize)]
    #[allow(clippy::upper_case_acronyms)]
    pub enum Protocol {
        HTTP,
        MQTT,
    }

    #[derive(Debug, Clone, serde::Serialize)]
    pub enum Action {
        AddDevice {
            topic: String,
            props: Vec<(String, String)>,
        },
        AddService {
            topic: String,
            props: Vec<(String, String)>,
        },
        RemDevice {
            topic: String,
        },
        RemService {
            topic: String,
        },
    }

    impl Action {
        pub fn target(&self) -> &str {
            match self {
                Action::AddDevice { topic, .. }
                | Action::AddService { topic, .. }
                | Action::RemDevice { topic }
                | Action::RemService { topic } => topic.as_ref(),
            }
        }

        fn parent(&self) -> Option<(&str, &str)> {
            match self {
                Action::AddDevice { topic, .. }
                | Action::AddService { topic, .. }
                | Action::RemDevice { topic }
                | Action::RemService { topic } => {
                    let len = topic.len();
                    let topic = topic.as_str();
                    match len {
                        0 => None,
                        1 => Some(("main", &topic[0..1])),
                        _ => Some((&topic[0..(len - 1)], &topic[(len - 1)..len])),
                    }
                }
            }
        }

        pub fn topic_id(&self) -> EntityTopicId {
            match (self.parent(), &self) {
                (None, _) => EntityTopicId::default_main_device(),
                (Some(_), Action::AddDevice { topic, .. })
                | (Some(_), Action::RemDevice { topic }) => {
                    format!("device/{topic}//").parse().unwrap()
                }
                (Some((parent, id)), Action::AddService { .. })
                | (Some((parent, id)), Action::RemService { .. }) => {
                    format!("device/{parent}/service/{id}").parse().unwrap()
                }
            }
        }

        pub fn parent_topic_id(&self) -> Option<EntityTopicId> {
            self.parent()
                .map(|(parent, _)| format!("device/{parent}//").parse().unwrap())
        }

        pub fn target_type(&self) -> EntityType {
            match (self.parent(), &self) {
                (None, _) => EntityType::MainDevice,

                (Some(_), Action::AddDevice { .. }) | (Some(_), Action::RemDevice { .. }) => {
                    EntityType::ChildDevice
                }

                (Some(_), Action::AddService { .. }) | (Some(_), Action::RemService { .. }) => {
                    EntityType::Service
                }
            }
        }

        pub fn properties(&self) -> serde_json::Map<String, serde_json::Value> {
            match self {
                Action::AddDevice { props, .. } | Action::AddService { props, .. } => props
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect(),

                Action::RemDevice { .. } | Action::RemService { .. } => serde_json::Map::new(),
            }
        }

        pub fn payload(&self) -> serde_json::Map<String, serde_json::Value> {
            let mut props = self.properties();
            if let Some(parent) = self.parent_topic_id() {
                props.insert("@parent".to_string(), parent.to_string().into());
            }
            props.insert("@type".to_string(), self.target_type().to_string().into());
            props
        }
    }

    impl From<Action> for EntityRegistrationMessage {
        fn from(action: Action) -> Self {
            EntityRegistrationMessage {
                topic_id: action.topic_id(),
                external_id: None,
                r#type: action.target_type(),
                parent: action.parent_topic_id(),
                twin_data: action.properties(),
            }
        }
    }

    impl From<Action> for EntityStoreRequest {
        fn from(action: Action) -> Self {
            match &action {
                Action::AddDevice { .. } | Action::AddService { .. } => {
                    let registration = EntityRegistrationMessage::from(action);
                    EntityStoreRequest::Create(registration)
                }

                Action::RemDevice { .. } | Action::RemService { .. } => {
                    EntityStoreRequest::Delete(action.topic_id())
                }
            }
        }
    }

    impl From<Action> for MqttMessage {
        fn from(action: Action) -> Self {
            let schema = MqttSchema::default();
            match &action {
                Action::AddDevice { .. } | Action::AddService { .. } => {
                    EntityRegistrationMessage::from(action).to_mqtt_message(&schema)
                }

                Action::RemDevice { .. } | Action::RemService { .. } => {
                    let topic = schema.topic_for(&action.topic_id(), &Channel::EntityMetadata);
                    MqttMessage::new(&topic, "")
                }
            }
        }
    }

    impl From<(Protocol, Action)> for EntityStoreRequest {
        fn from((protocol, action): (Protocol, Action)) -> Self {
            match protocol {
                Protocol::HTTP => EntityStoreRequest::from(action),
                Protocol::MQTT => EntityStoreRequest::MqttMessage(MqttMessage::from(action)),
            }
        }
    }

    type PropMap = serde_json::Map<String, serde_json::Value>;

    pub struct State {
        entities: HashMap<EntityTopicId, (EntityType, Option<EntityTopicId>, PropMap)>,
        registered: HashSet<EntityTopicId>,
    }

    impl State {
        pub fn new() -> Self {
            let mut state = State {
                entities: HashMap::default(),
                registered: HashSet::default(),
            };
            state.apply(
                Protocol::HTTP,
                Action::AddDevice {
                    topic: "".to_string(),
                    props: vec![],
                },
            );
            state
        }

        pub fn entity_topic_ids(&self) -> impl Iterator<Item = &EntityTopicId> {
            self.entities
                .keys()
                .filter(|topic| self.is_registered(topic))
        }

        pub fn get(
            &self,
            topic: &EntityTopicId,
        ) -> Option<&(EntityType, Option<EntityTopicId>, PropMap)> {
            self.entities.get(topic)
        }

        pub fn is_registered(&self, topic: &EntityTopicId) -> bool {
            self.registered.contains(topic)
        }

        pub fn apply(&mut self, protocol: Protocol, action: Action) -> HashSet<EntityTopicId> {
            let topic = action.topic_id();

            match action {
                Action::AddDevice { .. } | Action::AddService { .. } => {
                    let parent = action.parent_topic_id();

                    if let Some(parent) = parent.as_ref() {
                        if protocol == Protocol::HTTP && !self.registered.contains(parent) {
                            // Under HTTP, registering a child before its parent is an error
                            return HashSet::new();
                        }
                    }

                    if self.entities.contains_key(&topic) {
                        HashSet::new()
                    } else {
                        let entity_type = action.target_type();
                        self.entities.insert(
                            topic.clone(),
                            (entity_type, parent.clone(), action.properties()),
                        );

                        let new_entities = self.register(topic, parent);
                        if protocol == Protocol::HTTP {
                            new_entities
                        } else {
                            // Under MQTT, no response is sent back
                            HashSet::new()
                        }
                    }
                }

                Action::RemDevice { .. } | Action::RemService { .. } => {
                    if self.registered.contains(&topic) {
                        self.entities.remove(&topic);
                        self.registered.remove(&topic);

                        let old_entities = self.cascade_deregistration(HashSet::from([topic]));
                        if protocol == Protocol::HTTP {
                            old_entities
                        } else {
                            // Under MQTT, no response is sent back
                            HashSet::new()
                        }
                    } else {
                        HashSet::new()
                    }
                }
            }
        }

        fn register(
            &mut self,
            new_entity: EntityTopicId,
            parent: Option<EntityTopicId>,
        ) -> HashSet<EntityTopicId> {
            if parent
                .as_ref()
                .map_or(true, |p| self.registered.contains(p))
            {
                self.registered.insert(new_entity.clone());
                let new_entities = HashSet::from([new_entity]);
                self.cascade_registration(new_entities)
            } else {
                HashSet::new()
            }
        }

        fn cascade_registration(
            &mut self,
            mut new_entities: HashSet<EntityTopicId>,
        ) -> HashSet<EntityTopicId> {
            let mut new_connected = HashSet::new();
            for (entity_id, (_, parent, _)) in self.entities.iter() {
                if let Some(parent_id) = parent {
                    if !self.registered.contains(entity_id) && new_entities.contains(parent_id) {
                        new_connected.insert(entity_id.clone());
                    }
                }
            }

            if !new_connected.is_empty() {
                for entity_id in &new_connected {
                    self.registered.insert(entity_id.clone());
                }

                for entity_id in self.cascade_registration(new_connected) {
                    new_entities.insert(entity_id);
                }
            }

            new_entities
        }

        fn cascade_deregistration(
            &mut self,
            mut old_entities: HashSet<EntityTopicId>,
        ) -> HashSet<EntityTopicId> {
            let mut new_disconnected = HashSet::new();
            for (entity_id, (_, parent, _)) in self.entities.iter() {
                if let Some(parent_id) = parent {
                    if old_entities.contains(parent_id) {
                        new_disconnected.insert(entity_id.clone());
                    }
                }
            }

            if !new_disconnected.is_empty() {
                for entity_id in &new_disconnected {
                    self.entities.remove(entity_id);
                    self.registered.remove(entity_id);
                }

                for entity_id in self.cascade_deregistration(new_disconnected) {
                    old_entities.insert(entity_id);
                }
            }

            old_entities
        }
    }

    prop_compose! {
        pub fn random_protocol()(protocol in "[hm]") -> Protocol {
            if protocol == "h" {
                Protocol::HTTP
            } else {
                Protocol::MQTT
            }
        }
    }

    prop_compose! {
        pub fn random_name()(id in "[abc]{1,3}") -> String {
            id.to_string()
        }
    }

    prop_compose! {
        pub fn random_key()(id in "[xyz]") -> String {
            id.to_string()
        }
    }

    prop_compose! {
        pub fn random_value()(id in "[0-9]") -> String {
            id.to_string()
        }
    }

    prop_compose! {
        pub fn random_prop()(
            key in random_key(),
            value in random_value()
        ) -> (String,String) {
            (key, value)
        }
    }

    prop_compose! {
        pub fn random_props(max_length: usize)(
            vec in prop::collection::vec(random_prop(),
            0..max_length)
        ) -> Vec<(String,String)>
        {
            vec
        }
    }

    prop_compose! {
        pub fn pick_random_or_new(names: Vec<String>)(
            id in 0..(names.len()+1),
            name in random_name()
        ) -> String {
            names.get(id).map(|n| n.to_owned()).unwrap_or(name)
        }
    }

    prop_compose! {
        pub fn random_command_on(topic: String)(
            protocol in random_protocol(),
            action in 1..5,
            props in random_props(2)
        ) -> Command {
            let topic = topic.to_owned();
            let action = match action {
                1 => Action::AddDevice{ topic, props },
                2 => Action::AddService{ topic, props },
                3 => Action::RemService{ topic },
                _ => Action::RemDevice{ topic },
            };
            Command { protocol, action }
        }
    }

    pub fn random_command() -> impl Strategy<Value = Command> {
        random_name().prop_flat_map(random_command_on)
    }

    fn step(actions: Commands) -> impl Strategy<Value = Commands> {
        let nodes = actions
            .0
            .iter()
            .map(|c| c.action.target().to_owned())
            .collect();
        pick_random_or_new(nodes)
            .prop_flat_map(random_command_on)
            .prop_flat_map(move |action| {
                let mut actions = actions.clone();
                actions.0.push(action);
                Just(actions)
            })
    }

    pub fn walk(max_length: u32) -> impl Strategy<Value = Commands> {
        if max_length == 0 {
            Just(Commands(vec![])).boxed()
        } else if max_length == 1 {
            prop::collection::vec(random_command(), 0..=1)
                .prop_flat_map(|cmds| Just(Commands(cmds)))
                .boxed()
        } else {
            walk(max_length - 1).prop_flat_map(step).boxed()
        }
    }
}
