*** Settings ***
Library             JSONLibrary
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_flows


*** Test Cases ***
Add missing timestamps
    ${transformed_msg}    Execute Command
    ...    tedge flows test --processing-time "2025-06-27 11:31:02" te/device/main///m/ '{}'
    Should Contain    ${transformed_msg}    item="time":"2025-06-27T11:31:02.000Z"

Convert timestamps to ISO
    ${transformed_msg}    Execute Command    tedge flows test te/device/main///m/ '{"time": 1751023862.000}'
    Should Contain    ${transformed_msg}    item="time":"2025-06-27T11:31:02.000Z"

Convert message timestamps to local time
    Execute Command    ln -sf /usr/share/zoneinfo/Australia/Brisbane /etc/localtime
    ${transformed_msg}    Execute Command
    ...    tedge flows test --flow get-localtime.js --processing-time "2025-11-19T11:31:02" t {}
    Should Contain    ${transformed_msg}    item="time":"Wed Nov 19 2025 21:31:02 GMT+1000"
    Should Contain    ${transformed_msg}    item="utc":"2025-11-19T11:31:02.000Z"
    Should Contain    ${transformed_msg}    item="local":"2025-11-19T21:31:02.000
    [Teardown]    Execute Command    ln -sf /usr/share/zoneinfo/Etc/UTC /etc/localtime

Extract measurement type from topic
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"environment"

Use default measurement type
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/ '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"ThinEdgeMeasurement"

Translate complex tedge json to c8y json
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258, "location": {"latitude": 32.54, "longitude": -117.67, "altitude": 98.6 }, "pressure": 98}'
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] {"type":"environment","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":{"value":258}},"location":{"latitude":{"value":32.54},"longitude":{"value":-117.67},"altitude":{"value":98.6}},"pressure":{"pressure":{"value":98}}}

Using base64 to encode tedge flows input
    ${encoded_input}    Execute Command
    ...    echo -n '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258}' | base64 --wrap\=0
    ${transformed_msg}    Execute Command
    ...    tedge flows test --base64-input te/device/main///m/env ${encoded_input}
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] {"type":"env","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":{"value":258}}}

Using base64 to encode tedge flows output
    ${encoded_output}    Execute Command
    ...    echo -n '{"type":"env","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":{"value":258}}}' | base64 --wrap\=0
    ${transformed_msg}    Execute Command
    ...    tedge flows test --base64-output te/device/main///m/env '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258}'
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] ${encoded_output}

Units are configured using topic metadata
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/data/measurements.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/data/measurements.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Entity registration data are passed around using the context
    Install Flow    context-flows    device_registration.toml
    Install Flow    context-flows    device_events.toml
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/data/registrations.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/data/registrations.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}
    [Teardown]    Uninstall Flow    device_*.toml

Computing average over a time window
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/data/average.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval --flow /etc/tedge/flows/average.js
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/data/average.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Each instance of a script must have its own static state
    Install Flow    counting-flows    count-measurements.toml
    Install Flow    counting-flows    count-events.toml
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/data/count-messages.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval | sort
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/data/count-messages.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: ' | sort
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}
    [Teardown]    Uninstall Flow    count-*.toml

Running tedge-flows
    Install Flow    counting-flows    count-events.toml
    Execute Command    tedge mqtt sub test/count/e --duration 2s | grep '{}'
    [Teardown]    Uninstall Flow    count-events.toml

Consuming messages from a process stdout
    Install Flow    journalctl-flows    journalctl-follow.toml
    ${test_start}    Get Unix Timestamp
    Restart Service    tedge-agent
    ${messages}    Should Have MQTT Messages    topic=log/journalctl-follow    minimum=10    date_from=${test_start}
    Should Not Be Empty    ${messages[0]}    msg=Output should not be empty lines
    [Teardown]    Uninstall Flow    journalctl-follow.toml

Consuming messages from a process stdout, periodically
    Install Flow    journalctl-flows    journalctl-cursor.toml
    ${test_start}    Get Unix Timestamp
    Restart Service    tedge-agent
    ${messages}    Should Have MQTT Messages    topic=log/journalctl-cursor    minimum=1    date_from=${test_start}
    Should Not Be Empty    ${messages[0]}    msg=Output should not be empty lines
    [Teardown]    Uninstall Flow    journalctl-cursor.toml

Consuming messages from the tail of file
    Install Flow    input-flows    tail-named-pipe.toml
    ${start}    Get Unix Timestamp
    Execute Command    echo hello>/tmp/events
    Should Have MQTT Messages    topic=log/events    message_contains=hello    minimum=1    date_from=${start}
    [Teardown]    Uninstall Flow    tail-named-pipe.toml

Consuming messages from a file, periodically
    Install Flow    input-flows    read-file-periodically.toml
    Execute Command    echo hello >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep hello
    Execute Command    echo world >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep world
    Execute Command    rm /tmp/file.input
    Execute Command
    ...    tedge mqtt sub test/file/input --duration 1s | grep 'Fail to poll /tmp/file.input'
    Execute Command    echo 'hello world' >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep 'hello world'
    [Teardown]    Uninstall Flow    read-file-periodically.toml

Appending messages to a file
    Install Flow    input-flows    append-to-file.toml
    Execute Command    for i in $(seq 3); do tedge mqtt pub seq/events "$i"; done
    Execute Command    grep '\\[seq/events\\] 1' /tmp/events.log
    Execute Command    grep '\\[seq/events\\] 2' /tmp/events.log
    Execute Command    grep '\\[seq/events\\] 3' /tmp/events.log
    [Teardown]    Uninstall Flow    append-to-file.toml

Reloading a broken script when its permission is fixed
    # Break the a script and make sure tedge-flows can no more handle measurements
    Execute Command    chmod a-r /etc/tedge/flows/te_to_c8y.js
    Restart Service    tedge-flows
    ${transformed_msg}    Execute Command
    ...    sudo -u tedge tedge flows test te/device/main///m/ '{"temperature": 258}'
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain
    ...    ${transformed_msg}
    ...    item=Failed to compile flow /etc/tedge/flows/measurements.toml
    Should Contain
    ...    ${transformed_msg}
    ...    item=Cannot read file /etc/tedge/flows/te_to_c8y.js
    # Then fix the script
    Execute Command    chmod a+r /etc/tedge/flows/te_to_c8y.js
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 259.12}'
    Should Have MQTT Messages
    ...    topic=c8y/#
    ...    minimum=1
    ...    message_contains="temperature":{"temperature":{"value":259.12}}
    ...    date_from=${start}
    [Teardown]    Execute Command    cmd=chmod a-r /etc/tedge/flows/te_to_c8y.js

Publishing transformation errors
    Install Flow    input-flows    publish-js-errors.toml
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/foo 12345:6789
    Should Have MQTT Messages
    ...    topic=test/errors
    ...    minimum=1
    ...    message_contains=Error: Not a collectd topic
    ...    date_from=${start}

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/a/b/c foo,bar
    Should Have MQTT Messages
    ...    topic=test/errors
    ...    minimum=1
    ...    message_contains=Error: Not a collectd payload
    ...    date_from=${start}

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/a/b/c 12345:6789
    ${messages}    Should Have MQTT Messages
    ...    topic=test/output
    ...    minimum=1
    ...    message_contains=time
    ...    date_from=${start}
    ${message}    JSONLibrary.Convert String To Json    ${messages[0]}
    Should Be Equal As Integers    ${message["time"]}    12345
    Should Be Equal As Integers    ${message["b"]["c"]}    6789
    [Teardown]    Uninstall Flow    publish-js-errors.toml


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Copy Configuration Files
    Configure flows
    Start Service    tedge-flows

Copy Configuration Files
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/*.js    /etc/tedge/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/*.toml    /etc/tedge/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/*.samples    /etc/tedge/data/

Install Flow
    [Arguments]    ${directory}    ${definition_file}
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/${directory}/${definition_file}    /etc/tedge/flows/

    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-flows/status/flows
    ...    date_from=${start}
    ...    message_contains=${definition_file}

Uninstall Flow
    [Arguments]    ${definition_file}
    Execute Command    cmd=rm -f /etc/tedge/flows/${definition_file}

Configure flows
    # Required by tail-named-pipe.toml
    Execute Command    mkfifo /tmp/events
    Execute Command    chmod a+r /tmp/events
    # Required by journalctl.toml
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/journalctl' | sudo tee -a /etc/sudoers.d/tedge
