*** Settings ***
Resource    ../../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:operation
Test Teardown    Custom Teardown

*** Variables ***
${SMART_REST_ONE_TEMPLATES}=    SEPARATOR=\n
...    10,339,GET,/identity/externalIds/c8y_Serial/%%,,application/vnd.com.nsn.cumulocity.externalId+json,%%,STRING,
...    10,311,GET,/alarm/alarms?source\=%%&status\=%%&pageSize\=100,,,%%,UNSIGNED STRING,
...    11,800,$.managedObject,,$.id
...    11,808,$.alarms,,$.id,$.type

*** Test Cases ***

Supports SmartREST 1.0 Templates
    [Template]    Register and Use SmartREST 1.0. Templates
    use_builtin_bridge=true
    use_builtin_bridge=false

*** Keywords ***

Register and Use SmartREST 1.0. Templates
    [Arguments]    ${use_builtin_bridge}
    Custom Setup    use_builtin_bridge=${use_builtin_bridge}

    ${TEMPLATE_XID}=    Get Random Name    prefix=TST_Template
    Set Test Variable    $TEMPLATE_XID
    Execute Command    tedge config set c8y.smartrest1.templates "${TEMPLATE_XID}"
    Execute Command    tedge connect c8y    timeout=10
    ${mo}=    Device Should Exist                      ${DEVICE_SN}

    # register templates
    Execute Command    curl --max-time 15 -sf -XPOST http://127.0.0.1:8001/c8y/s -H "Content-Type: plain/text" -H "X-Id: ${TEMPLATE_XID}" --data "${SMART_REST_ONE_TEMPLATES}"

    # Use templates
    # Get managed object id
    Execute Command    cmd=tedge mqtt pub c8y/s/ul/${TEMPLATE_XID} '339,${DEVICE_SN}'
    Should Have MQTT Messages    c8y/s/dl/${TEMPLATE_XID}    message_pattern=^800,\\d+,${mo["id"]}    timeout=10

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

    Execute Command    tedge config set c8y.username "${CREDENTIALS.username}"    log_output=${False}
    Execute Command    tedge config set c8y.password "${CREDENTIALS.password}"    log_output=${False}
    Execute Command    cmd=printf 'C8Y_DEVICE_USER="%s"\nC8Y_DEVICE_PASSWORD="%s"\n' "${CREDENTIALS.username}" "${CREDENTIALS.password}" > /etc/tedge/c8y-mqtt.env

Register Device Using Bootstrap Credentials
    [Arguments]    ${SERIAL}

    # setup registration service
    Transfer To Device    ${CURDIR}/register-device.sh    /usr/bin/register-device.sh
    Transfer To Device    ${CURDIR}/register-device.service    /lib/systemd/system/register-device.service
    Execute Command    cmd=printf 'C8Y_BOOTSTRAP_USER=%s\nC8Y_BOOTSTRAP_PASSWORD=%s\n' '${C8Y_CONFIG.bootstrap_username}' '${C8Y_CONFIG.bootstrap_password}' > /etc/tedge/c8y-bootstrap.env    log_output=${False}
    Execute Command    systemctl daemon-reload

    # Start background registration service
    Execute Command    systemctl start register-device.service

    # Register device in the platform and then approve it (after the background service connects as well)
    Cumulocity.Register Device With Basic Auth    external_id=${SERIAL}

Custom Setup
    [Arguments]    ${use_builtin_bridge}
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    Execute Command    tedge config set mqtt.bridge.built_in ${use_builtin_bridge}

    # Allow mapper to read env variable from file
    Transfer To Device    ${CURDIR}/override.conf    /etc/systemd/system/tedge-mapper-c8y.service.d/override.conf
    Execute Command    systemctl daemon-reload

    Set Suite Variable    $DEVICE_SN

    Register Device    ${DEVICE_SN}

Custom Teardown
    Get Logs
    IF    $TEMPLATE_XID
        Delete SmartREST 1.0 Template    ${TEMPLATE_XID}
    END
