*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Custom Setup
Suite Teardown         Get Logs


*** Test Cases ***
Set keys should return value on stdout
    ${output}=    Execute Command    tedge config get device.id 2>/dev/null    strip=True
    Should Be Equal    ${output}    ${DEVICE_SN}

Unset keys should not return anything on stdout and warnings on stderr
    ${output}=    Execute Command    tedge config get az.url 2>/dev/null    exp_exit_code=0
    Should Be Empty    ${output}

    ${stderr}=    Execute Command    tedge config get az.url 2>&1 >/dev/null    exp_exit_code=0
    Should Not Be Empty    ${stderr}

Invalid keys should not return anything on stdout and warnings on stderr
    ${output}=    Execute Command    tedge config get does.not.exist 2>/dev/null    exp_exit_code=!0
    Should Be Empty    ${output}

    ${stderr}=    Execute Command    tedge config get does.not.exist 2>&1 >/dev/null    exp_exit_code=!0
    Should Not Be Empty    ${stderr}


Set configuration via environment variables
    [Template]    Check known tedge environment settings
    TEDGE_AZ_URL                az.url                   az.example.com
    TEDGE_C8Y_URL               c8y.url                  example.com
    TEDGE_DEVICE_KEY_PATH       device.key.path          /etc/example.key
    TEDGE_DEVICE_CERT_PATH      device.cert.path         /etc/example.pem
    TEDGE_MQTT_BIND_ADDRESS     mqtt.bind_address        0.0.0.1
    TEDGE_MQTT_CLIENT_HOST      mqtt.client.host         custom_host_name
    TEDGE_MQTT_CLIENT_PORT      mqtt.client.port         8888


Set unknown configuration via environment variables
    ${stdout}    ${stderr}=    Execute Command    env TEDGE_C8Y_UNKNOWN_CONFIGURATION\=dummy TEDGE_C8Y_URL\=example.com tedge config get c8y.url    stdout=${True}    stderr=${True}
    Should Be Equal    ${stdout}    example.com\n
    Should Contain    ${stderr}    Unknown configuration field "c8y_unknown_configuration" from environment variable TEDGE_C8Y_UNKNOWN_CONFIGURATION


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

Check known tedge environment settings
    [Arguments]    ${ENV_NAME}    ${KEY_NAME}    ${VALUE}
    ${output}=    Execute Command    env ${ENV_NAME}\=${VALUE} tedge config get ${KEY_NAME}
    Should Be Equal    ${output}    ${VALUE}\n
