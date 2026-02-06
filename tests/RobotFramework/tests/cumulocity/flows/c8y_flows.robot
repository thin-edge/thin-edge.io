*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
Extend C8Y mapper with user-provided flows
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/collectd.js    /etc/tedge/mappers/c8y/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/collectd.toml    /etc/tedge/mappers/c8y/flows/
    Wait For The Flow To Reload    ${start}    collectd.toml

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/a/b/c 12345:6789
    ${message}    Should Have MQTT Messages
    ...    topic=c8y/measurement/measurements/create
    ...    message_contains="type":"collectd"

    Cumulocity.Set Managed Object    ${DEVICE_SN}
    ${measurements}    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=collectd
    ...    value=b
    ...    series=c
    Should Be Equal As Numbers    ${measurements[0]["b"]["c"]["value"]}    6789.0

Get entity metadata from the c8y mapper context
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements.js    /etc/tedge/mappers/c8y/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements.toml    /etc/tedge/mappers/c8y/flows/
    Wait For The Flow To Reload    ${start}    custom-measurements.toml

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub custom/device/child///m/temperature 23.1
    ${message}    Should Have MQTT Messages
    ...    topic=c8y/measurement/measurements/create
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "type":"custom"
    Should Contain    ${message}[0]    "externalId":"${CHILD_SN}"

    Cumulocity.Set Managed Object    ${CHILD_SN}
    ${measurements}    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    value=temperature
    ...    series=temperature
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    23.1

On start builtin-flows TOML files are generated
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/measurements.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/events.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/alarms.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/units.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/health.toml

The builtin-flows TOML files can disabled
    Execute Command    tedge config set c8y.topics 'te/+/+/+/+,te/+/+/+/+/twin/+'
    Restart Service    tedge-mapper-c8y
    # The files are generated
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/measurements.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/events.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/alarms.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/units.toml
    Execute Command    ls -lh /etc/tedge/mappers/c8y/flows/health.toml
    # But with no inputs
    Execute Command    cmd=grep 'input.mqtt.topics = \\[]' /etc/tedge/mappers/c8y/flows/measurements.toml
    Execute Command    cmd=grep 'input.mqtt.topics = \\[]' /etc/tedge/mappers/c8y/flows/events.toml
    Execute Command    cmd=grep 'input.mqtt.topics = \\[]' /etc/tedge/mappers/c8y/flows/alarms.toml
    Execute Command    cmd=grep 'input.mqtt.topics = \\[]' /etc/tedge/mappers/c8y/flows/units.toml
    Execute Command    cmd=grep 'input.mqtt.topics = \\[]' /etc/tedge/mappers/c8y/flows/health.toml
    [Teardown]    Restore Builtin Flows

The builtin-flows TOML files can be restored
    # Updating c8y.topics but not mqtt.topic_root
    Execute Command
    ...    tedge config set c8y.topics 'te2/+/+/+/+,te2/+/+/+/+/twin/+,te2/+/+/+/+/m/+,te2/+/+/+/+/m/+/meta,te2/+/+/+/+/e/+,te2/+/+/+/+/a/+,te2/+/+/+/+/status/health'
    Restart Service    tedge-mapper-c8y
    # The flows have no inputs because of te2 not being the mqtt.topic_root
    Execute Command    grep te2/+/+/+/+/m/+ /etc/tedge/mappers/c8y/flows/measurements.toml    exp_exit_code=1
    Execute Command    grep te2/+/+/+/+/e/+ /etc/tedge/mappers/c8y/flows/events.toml    exp_exit_code=1
    Execute Command    grep te2/+/+/+/+/a/+ /etc/tedge/mappers/c8y/flows/alarms.toml    exp_exit_code=1
    Execute Command    grep te2/+/+/+/+/m/+/meta /etc/tedge/mappers/c8y/flows/units.toml    exp_exit_code=1
    Execute Command    grep te2/+/+/+/+/status/health /etc/tedge/mappers/c8y/flows/health.toml    exp_exit_code=1

    # Fixing the mistake
    Execute Command    tedge config set mqtt.topic_root te2
    Restart Service    tedge-mapper-c8y

    Execute Command    grep te2/+/+/+/+/m/+ /etc/tedge/mappers/c8y/flows/measurements.toml
    Execute Command    grep te2/+/+/+/+/e/+ /etc/tedge/mappers/c8y/flows/events.toml
    Execute Command    grep te2/+/+/+/+/a/+ /etc/tedge/mappers/c8y/flows/alarms.toml
    Execute Command    grep te2/+/+/+/+/m/+/meta /etc/tedge/mappers/c8y/flows/units.toml
    Execute Command    grep te2/+/+/+/+/status/health /etc/tedge/mappers/c8y/flows/health.toml

    [Teardown]    Restore Builtin Flows


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}-child
    Execute Command    tedge mqtt pub --retain 'te/device/child//' '{"@type":"child-device","@id":"${CHILD_SN}"}'
    Device Should Exist    ${CHILD_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y

Custom Teardown
    Get Suite Logs

Wait For The Flow To Reload
    [Arguments]    ${start}    ${flow}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/status/flows
    ...    date_from=${start}
    ...    message_contains=${flow}

Restore builtin flows
    Execute Command    tedge config set mqtt.topic_root te
    Execute Command
    ...    tedge config set c8y.topics 'te/+/+/+/+,te/+/+/+/+/twin/+,te/+/+/+/+/m/+,te/+/+/+/+/m/+/meta,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health'
    Restart Service    tedge-mapper-c8y
