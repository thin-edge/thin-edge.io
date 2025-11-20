*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:operation


*** Variables ***
${SMART_REST_ONE_TEMPLATES}=
...                             SEPARATOR=\n
...                             10,339,GET,/identity/externalIds/c8y_Serial/%%,,application/vnd.com.nsn.cumulocity.externalId+json,%%,STRING,
...                             10,311,GET,/alarm/alarms?source\=%%&status\=%%&pageSize\=100,,,%%,UNSIGNED STRING,
...                             11,800,$.managedObject,,$.id
...                             11,808,$.alarms,,$.id,$.type


*** Test Cases ***
Supports SmartREST 1.0 Templates - builtin
    Register and Use SmartREST 1.0. Templates    use_builtin_bridge=true

Supports SmartREST 1.0 Templates - mosquitto
    [Tags]    test:retry(1)    workaround    # rarely no message arrives on c8y/s/dl/template
    Register and Use SmartREST 1.0. Templates    use_builtin_bridge=false

tedge connect c8y --test works with basic auth
    [Tags]    \#3791
    Register and Use SmartREST 1.0. Templates    use_builtin_bridge=true
    Execute Command    tedge connect c8y --test
    Should Have MQTT Messages    c8y/s/ds    message_contains=124


*** Keywords ***
Register and Use SmartREST 1.0. Templates
    [Arguments]    ${use_builtin_bridge}
    Custom Setup    use_builtin_bridge=${use_builtin_bridge}

    # device.id should be set by tedge config and confirm the test doesn't use certificate
    Execute Command    tedge config set device.id ${DEVICE_SN}
    Execute Command    tedge cert remove
    File Should Not Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Not Exist    /etc/tedge/device-certs/tedge-private-key.pem

    ${TEMPLATE_XID}=    Get Random Name    prefix=TST_Template
    Set Test Variable    $TEMPLATE_XID
    Execute Command    tedge config set c8y.smartrest1.templates "${TEMPLATE_XID}"
    Execute Command    tedge connect c8y
    ${mo}=    Device Should Exist    ${DEVICE_SN}

    # register templates
    Execute Command
    ...    curl --max-time 15 -sf -XPOST http://127.0.0.1:8001/c8y/s -H "Content-Type: plain/text" -H "X-Id: ${TEMPLATE_XID}" --data "${SMART_REST_ONE_TEMPLATES}"

    SmartREST1 Template Should Exist    ${TEMPLATE_XID}

    # Since we create a SmartREST template after initial connection, reconnect is required to subscribe the template properly.
    Execute Command    tedge reconnect c8y

    # Use templates
    # Get managed object id
    Execute Command    cmd=tedge mqtt pub --qos 1 c8y/s/ul/${TEMPLATE_XID} '339,${DEVICE_SN}'
    Should Have MQTT Messages    c8y/s/dl/${TEMPLATE_XID}    message_pattern=^800,\\d+,${mo["id"]}

    Execute Command    cmd=tedge mqtt pub te/device/main///a/test '{"text":"test alarm","severity":"major"}' -r
    Device Should Have Alarm/s    type=test    expected_text=test alarm

    # Get alarms
    Execute Command    cmd=tedge mqtt pub c8y/s/ul/${TEMPLATE_XID} '311,${mo["id"]},ACTIVE'
    Should Have MQTT Messages    c8y/s/dl/${TEMPLATE_XID}    message_pattern=^808,\\d+,\\d+,test    timeout=10

    # Operations
    ${OPERATION}=    Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}

Register Device
    [Arguments]    ${SERIAL}
    ${CREDENTIALS}=    Cumulocity.Bulk Register Device With Basic Auth    external_id=${SERIAL}

    Execute Command
    ...    cmd=printf '[c8y]\nusername = "%s"\npassword = "%s"\n' '${CREDENTIALS.username}' '${CREDENTIALS.password}' > /etc/tedge/credentials.toml

SmartREST1 Template Should Exist
    [Arguments]    ${name}
    Execute Command
    ...    cmd=curl --max-time 15 -sf -X GET http://127.0.0.1:8001/c8y/identity/externalIds/c8y_SmartRestDeviceIdentifier/${name}

Custom Setup
    [Arguments]    ${use_builtin_bridge}
    ${DEVICE_SN}=    Setup    register=${False}
    Set Cumulocity URLs
    Execute Command    tedge config set mqtt.bridge.built_in ${use_builtin_bridge}
    Execute Command    tedge config set c8y.auth_method basic

    Set Suite Variable    $DEVICE_SN

    Register Device    ${DEVICE_SN}

Custom Teardown
    Get Logs
    IF    $TEMPLATE_XID    Delete SmartREST 1.0 Template    ${TEMPLATE_XID}
