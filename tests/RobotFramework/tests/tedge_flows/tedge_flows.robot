*** Settings ***
Library             JSONLibrary
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_flows


*** Test Cases ***
Add missing timestamps
    ${transformed_msg}    Execute Command    tedge flows test te/device/main///m/ '{}'
    Should Contain    ${transformed_msg}    item=time

Convert timestamps to ISO
    ${transformed_msg}    Execute Command    tedge flows test te/device/main///m/ '{"time": 1751023862.000}'
    Should Contain    ${transformed_msg}    item="time":"2025-06-27T11:31:02.000Z"

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
    ...    [c8y/measurement/measurements/create] {"type":"environment","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":258},"location":{"latitude":32.54,"longitude":-117.67,"altitude":98.6},"pressure":{"pressure":98}}

Using base64 to encode tedge flows input
    ${encoded_input}    Execute Command
    ...    echo -n '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258}' | base64 --wrap\=0
    ${transformed_msg}    Execute Command
    ...    tedge flows test --base64-input te/device/main///m/env ${encoded_input}
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] {"type":"env","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":258}}

Using base64 to encode tedge flows output
    ${encoded_output}    Execute Command
    ...    echo -n '{"type":"env","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":258}}' | base64 --wrap\=0
    ${transformed_msg}    Execute Command
    ...    tedge flows test --base64-output te/device/main///m/env '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258}'
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] ${encoded_output}

Units are configured using topic metadata
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/measurements.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/measurements.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Computing average over a time window
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/average.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval --flow /etc/tedge/flows/average.js
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/average.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Each instance of a script must have its own static state
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/count-messages.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval | sort
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/count-messages.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: ' | sort
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Running tedge-flows
    # Assuming the flow count-events.toml has been properly installed
    Execute Command    tedge mqtt sub test/count/e --duration 2s | grep '{}'

Consuming messages from a process stdout
    # Assuming the flow journalctl-follow.toml has been properly installed
    Install Journalctl Flow    journalctl-follow.toml
    ${test_start}    Get Unix Timestamp
    Restart Service    tedge-agent
    ${messages}    Should Have MQTT Messages    topic=log/journalctl-follow    minimum=10    date_from=${test_start}
    Should Not Be Empty    ${messages[0]}    msg=Output should not be empty lines
    [Teardown]    Uninstall Journalctl Flow    journalctl-follow.toml

Consuming messages from a process stdout, periodically
    # Assuming the flow journalctl-cursor.toml has been properly installed
    Install Journalctl Flow    journalctl-cursor.toml
    ${test_start}    Get Unix Timestamp
    Restart Service    tedge-agent
    ${messages}    Should Have MQTT Messages    topic=log/journalctl-cursor    minimum=1    date_from=${test_start}
    Should Not Be Empty    ${messages[0]}    msg=Output should not be empty lines
    [Teardown]    Uninstall Journalctl Flow    journalctl-cursor.toml

Consuming messages from the tail of file
    # Assuming the flow tail-named-pipe.toml has been properly installed
    ${start}    Get Unix Timestamp
    Execute Command    echo hello>/tmp/events
    Should Have MQTT Messages    topic=log/events    message_contains=hello    minimum=1    date_from=${start}

Consuming messages from a file, periodically
    # Assuming the flow read-file-periodically.toml has been properly installed
    Execute Command    echo hello >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep hello
    Execute Command    echo world >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep world
    Execute Command    rm /tmp/file.input
    Execute Command
    ...    tedge mqtt sub test/file/input --duration 1s | grep 'Error in /etc/tedge/flows/read-file-periodically.toml'
    Execute Command    echo 'hello world' >/tmp/file.input
    Execute Command    tedge mqtt sub test/file/input --duration 1s | grep 'hello world'

Appending messages to a file
    # Assuming the flow append-to-file.toml has been properly installed
    Execute Command    for i in $(seq 3); do tedge mqtt pub seq/events "$i"; done
    Execute Command    grep '\\[seq/events\\] 1' /tmp/events.log
    Execute Command    grep '\\[seq/events\\] 2' /tmp/events.log
    Execute Command    grep '\\[seq/events\\] 3' /tmp/events.log

Publishing transformation errors
    # Assuming the flow publish-js-errors.toml has been properly installed
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


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Copy Configuration Files
    Configure flows
    Start Service    tedge-flows

Copy Configuration Files
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/*    /etc/tedge/flows/

Install Journalctl Flow
    [Arguments]    ${definition_file}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/journalctl-flows/${definition_file}    /etc/tedge/flows/

Uninstall Journalctl Flow
    [Arguments]    ${definition_file}
    Execute Command    cmd=rm -f /etc/tedge/flows/${definition_file}

Configure flows
    # Required by tail-named-pipe.toml
    Execute Command    mkfifo /tmp/events
    Execute Command    chmod a+r /tmp/events
    # Required by journalctl.toml
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/journalctl' | sudo tee -a /etc/sudoers.d/tedge
