// Translate a collectd measurement into a thin-edge measurement

const utf8 = new TextDecoder();

export function onMessage(message) {
  let groups = message.topic.split("/");
  let data = utf8.decode(message.payload).split(":");

  if (groups.length < 4) {
    throw new Error("Not a collectd topic");
  }

  if (data.length < 2) {
    throw new Error("Not a collectd payload");
  }

  let group = groups[2];
  let measurement = groups[3];
  let time = data[0];
  let value = data[1];

  return [
    {
      topic: "te/device/main///m/collectd",
      payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`,
    },
  ];
}
