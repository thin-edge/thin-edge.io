export function bridge_config(connection, config) {
  const rules = [
    {
      direction: "outbound",
      topic: "s/us/#",
      enabled: true,
    },
    ...["s", "q", "c", "t"].map(mode => ({
      direction: "outbound",
      topic: `${mode}/ul/#`,
      enabled: connection.auth_method === "password",
    })),

    ...config.c8y.smartrest1.templates.map(template => ({
      direction: "inbound",
      topic: `s/dl/${template}`,
      enabled: true,
    })),

    {
      direction: "outbound",
      topic: "s/ut/#",
      enabled: true,
    },
    {
      direction: "outbound",
      topic: "inventory/managedObjects/update/#",
      enabled: true,
    },
    {
      direction: "inbound",
      topic: "devicecontrol/notifications",
      enabled: true,
    },
    {
      direction: "inbound",
      topic: "error",
      enabled: true,
    },
    {
      direction: "inbound",
      topic: "s/ds",
      enabled: true,
    },
    {
      direction: "inbound",
      topic: "s/dt",
      enabled: true,
    },
    {
      direction: "inbound",
      topic: "s/e",
      enabled: true,
    },


    // JWT retrieval topics
    {
      direction: "outbound",
      topic: "s/uat",
      enabled: connection.auth_method == "certificate",
    },
    {
      direction: "inbound",
      topic: "s/dat",
      enabled: connection.auth_method == "certificate",
    },
    
    ...["create", "createBulk"].flatMap(action => ["alarm", "event", "measurement"].map(ty => ({
      direction: "outbound",
      topic: `${ty}/${ty}s/${action}/#`,
      enabled: true
    }))),

    // mqtt service topics
    {
      local_prefix: `${config.c8y.bridge.topic_prefix}/mqtt/out/`,
      topic: "#",
      direction: "outbound",
      enabled: config.c8y.mqtt_service.enabled,
    },

    ...config.c8y.mqtt_service.topics.map(topic => ({
      local_prefix: `${config.c8y.bridge.topic_prefix}/mqtt/in/`,
      topic,
      direction: "inbound",
      enabled: config.c8y.mqtt_service.enabled,
    })),

    // SmartREST message including different processing modes
    ...["s","t","q","c"].map((item) => ({
      direction: "outbound",
      topic: `${item}/us/#`,
      enabled: true,
    })),

    // Custom SmartREST templates
    ...(config.c8y.smartrest.templates || []).map((item) => ({
      direction: "inbound",
      topic: `s/dc/${item}`,
      enabled: true,
    })),

    ...["s","t","q","c"].map((item) => ({
      direction: "outbound",
      topic: `${item}/uc/#`,
      enabled: true,
    })),
  ];

  // return full configuration object
  // as json
  return JSON.stringify({
    local_prefix: `${config.c8y.bridge.topic_prefix}/`,
    remote_prefix: "",
    rule: rules.filter(item => item.enabled),
  });
}