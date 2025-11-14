*** Settings ***
Resource            ../../resources/common.resource
Library             DateTime
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Get Logs


*** Test Cases ***
Connect to Cumulocity MQTT Service endpoint
    Skip    Until SmartREST proxy for MQTT service is enabled on thin-edge-io.eu-latest tenant
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    Execute Command    tedge mqtt pub c8y/custom/out/test/topic '"hello"'
    # TODO: Validate message received on test/topic on C8Y

    Sleep    1s

Connect to Cumulocity MQTT Service endpoint basic auth
    Skip    Until SmartREST proxy for MQTT service is enabled on thin-edge-io.eu-latest tenant
    ${DEVICE_SN}=    Setup    register=${False}

    Execute Command    tedge config set device.id ${DEVICE_SN}
    Execute Command    tedge config set c8y.url "${C8Y_CONFIG.host}"
    Execute Command    tedge config set c8y.mqtt_service.enabled true

    Execute Command
    ...    cmd=printf '[c8y]\nusername = "%s"\npassword = "%s"\n' '${C8Y_CONFIG.tenant}/${C8Y_CONFIG.username}' '${C8Y_CONFIG.password}' > /etc/tedge/credentials.toml
    Execute Command    tedge config set c8y.auth_method basic

    Execute Command    tedge connect c8y

    # TODO: Subscribing to test/topic from another client
    Execute Command    tedge mqtt pub c8y-mqtt/test/topic '"hello"'
    # TODO: Validate message received on test/topic on the other client

    Sleep    1s

Connect to Cumulocity MQTT Service endpoint builtin bridge
    Skip    Until SmartREST proxy for MQTT service is enabled on thin-edge-io.eu-latest tenant
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    Execute Command    tedge mqtt pub c8y/custom/out/test/topic '"hello"'
    # TODO: Validate message received on test/topic on C8Y

    Sleep    1s
