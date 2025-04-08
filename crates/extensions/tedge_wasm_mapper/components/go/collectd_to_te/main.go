package main

import (
    "example.com/internal/tedge/filter/tedge"
    "fmt"
    "strings"

    // See https://github.com/bytecodealliance/go-modules/blob/main/cm/README.md
    "go.bytecodealliance.org/cm"
)

type Message = tedge.Message
type DateTime = tedge.DateTime
type FilterError = tedge.FilterError
type FilterErrorShape = tedge.FilterErrorShape
type MessageList = cm.List[Message]
type MessageListResult = cm.Result[FilterErrorShape, cm.List[Message], FilterError]

func init() {
    // Process a single message; producing zero, one or more transformed messages
    //
    //	process: func(timestamp: datetime, message: message) -> result<list<message>, filter-error>
    tedge.Exports.Process = func(timestamp DateTime, message Message) MessageListResult {
	    groups := strings.Split(message.Topic, "/");
	    data := strings.Split(message.Payload, ":");

	    group := groups[2];
	    measurement := groups[3];
	    time := data[0];
	    value := data[1];

	    topic := "te/main/device///m/collectd";
	    payload := fmt.Sprintf("{\"time\": %s, %q: {%q: %s} } ", time, group, measurement, value)
	    

	    messages := []Message{ Message { Topic: topic, Payload: payload }};

	    return cm.OK[MessageListResult](cm.ToList(messages));
    }
}

// main is required for the `wasi` target, even if it isn't used.
func main() {}
