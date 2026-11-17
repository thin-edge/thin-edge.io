use super::*;
use test_case::test_case;

#[tokio::test]
async fn software_list_is_requested_when_software_update_capability_is_received() {
    let mut test = SoftwareListTest::new();

    test.receive(test.capability()).await;
    test.take_software_list_requests();

    let requests = test.software_list_requests();
    assert_eq!(requests.len(), 1, "Expected a software list request");
    assert_eq!(
        serde_json::from_slice::<Value>(requests[0].payload_bytes()).unwrap(),
        json!({"status": "init"})
    );
}

#[test_case(CommandStatus::Init)]
#[test_case(CommandStatus::Executing)]
#[test_case(CommandStatus::Successful)]
#[tokio::test]
async fn software_list_is_not_requested_while_a_previous_request_is_pending(status: CommandStatus) {
    let mut test = SoftwareListTest::new();

    // A software list command issued by this mapper, e.g. before being restarted,
    // received as a retained message from the broker and not cleared yet
    let previous_request = test.request("c8y-mapper-1234").with_status(status);
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.receive(test.capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(0);
}

#[tokio::test]
async fn software_list_is_not_requested_when_a_pending_request_is_received_after_the_capability() {
    let mut test = SoftwareListTest::new();

    test.receive(test.capability()).await;
    let previous_request = test.request("c8y-mapper-1234");
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.take_software_list_requests();

    test.assert_software_list_requests(0);
}

#[tokio::test]
async fn software_list_is_requested_once_when_the_capability_is_received_repeatedly() {
    let mut test = SoftwareListTest::new();

    test.receive(test.capability()).await;
    test.take_software_list_requests();
    test.receive(test.capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

#[tokio::test]
async fn software_list_is_requested_once_the_previous_request_is_cleared() {
    let mut test = SoftwareListTest::new();

    let previous_request = test
        .request("c8y-mapper-1234")
        .with_status(CommandStatus::Successful);
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.receive(previous_request.clearing_message(&test.schema()))
        .await;
    test.receive(test.capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

#[tokio::test]
async fn software_list_is_requested_when_the_agent_restarts_with_a_request_still_pending() {
    let mut test = SoftwareListTest::new();

    let previous_request = test.request("c8y-mapper-1234");
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.receive(test.live_capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

#[tokio::test]
async fn software_list_is_requested_when_the_agent_restarts_and_resumes_a_cleared_request() {
    let mut test = SoftwareListTest::new();

    let previous_request = test
        .request("c8y-mapper-1234")
        .with_status(CommandStatus::Successful);
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.receive(previous_request.clearing_message(&test.schema()))
        .await;
    // Republished by an agent restarted before having seen the clearing message
    test.receive(previous_request.command_message(&test.schema()))
        .await;
    test.receive(test.live_capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

#[tokio::test]
async fn software_list_request_not_issued_by_the_mapper_does_not_prevent_a_new_request() {
    let mut test = SoftwareListTest::new();

    // A software list command issued locally, e.g. using `tedge mqtt pub`
    let local_request = test.request("local-1111");
    test.receive(local_request.command_message(&test.schema()))
        .await;
    test.receive(test.capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

#[tokio::test]
async fn software_list_request_pending_for_another_device_does_not_prevent_a_new_request() {
    let mut test = SoftwareListTest::new();
    let child = EntityTopicId::default_child_device("child1").unwrap();

    let child_request =
        SoftwareListCommand::new(&child, "c8y-mapper-1234".into()).command_message(&test.schema());
    register_source_entities(&child_request.topic.name, &mut test.converter).await;
    test.receive(child_request).await;
    test.receive(test.capability()).await;
    test.take_software_list_requests();

    test.assert_software_list_requests(1);
}

/// A converter receiving software list messages, collecting the messages it publishes
struct SoftwareListTest {
    _tmp_dir: TempTedgeDir,
    _http_proxy: FakeServerBox<HttpRequest, HttpResult>,
    converter: CumulocityConverter,
    device: EntityTopicId,
    published: Vec<MqttMessage>,
}

impl SoftwareListTest {
    fn new() -> Self {
        let tmp_dir = TempTedgeDir::new();
        let (converter, http_proxy) = create_c8y_converter(&tmp_dir);
        SoftwareListTest {
            _tmp_dir: tmp_dir,
            _http_proxy: http_proxy,
            converter,
            device: EntityTopicId::default_main_device(),
            published: vec![],
        }
    }

    fn schema(&self) -> MqttSchema {
        self.converter.mqtt_schema.clone()
    }

    /// A software list request to the main device
    fn request(&self, cmd_id: &str) -> SoftwareListCommand {
        SoftwareListCommand::new(&self.device, cmd_id.into())
    }

    /// The retained software update capability of the main device
    fn capability(&self) -> MqttMessage {
        SoftwareUpdateCommand::capability_message(&self.schema(), &self.device)
    }

    /// The software update capability published by an agent that has just started
    fn live_capability(&self) -> MqttMessage {
        let mut capability = self.capability();
        capability.retain = false;
        capability
    }

    async fn receive(&mut self, message: MqttMessage) {
        let published = self.converter.try_convert(&message).await.unwrap();
        self.published.extend(published);
    }

    fn take_software_list_requests(&mut self) {
        let requests = self.converter.take_software_list_requests();
        self.published.extend(requests);
    }

    /// The software list requests published to the main device
    fn software_list_requests(&self) -> Vec<&MqttMessage> {
        let software_list_topics = self.converter.mqtt_schema.topics(
            EntityFilter::Entity(&self.device),
            ChannelFilter::Command(OperationType::SoftwareList),
        );
        self.published
            .iter()
            .filter(|message| software_list_topics.accept(message))
            .collect()
    }

    #[track_caller]
    fn assert_software_list_requests(&self, expected: usize) {
        assert_eq!(
            self.software_list_requests().len(),
            expected,
            "Unexpected software list requests in {:?}",
            self.published
        );
    }
}
