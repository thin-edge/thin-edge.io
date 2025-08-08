*** Settings ***
Resource            ../../resources/common.resource
Library             DateTime
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Get Logs


*** Test Cases ***
Connect to Cumulocity MQTT Service endpoint
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    External Identity Should Exist    ${DEVICE_SN}:device:main:service:mosquitto-c8y-mqtt-bridge    show_info=False
    Device Should Have Fragment Values    status\=up

    Execute Command    tedge mqtt pub c8y-mqtt/test/topic '"hello"'
    # TODO: Validate message received on test/topic on C8Y

    Sleep    1s

Connect to Cumulocity MQTT Service endpoint basic auth
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
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics demo/topic
    Execute Command    tedge connect c8y

    External Identity Should Exist    ${DEVICE_SN}:device:main:service:tedge-mapper-bridge-c8y-mqtt    show_info=False
    Device Should Have Fragment Values    status\=up

    Execute Command    tedge mqtt pub c8y-mqtt/test/topic '"hello"'
    # TODO: Validate message received on test/topic on C8Y

    Sleep    1s
