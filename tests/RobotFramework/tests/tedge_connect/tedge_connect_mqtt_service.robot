*** Settings ***
Resource            ../../resources/common.resource
Library             DateTime
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Get Logs


*** Test Cases ***
Connect to Cumulocity MQTT Service endpoint mosquitto bridge
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set mqtt.bridge.built_in false
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    Verify Custom Topic Publish

Connect to Cumulocity MQTT Service endpoint basic auth
    ${DEVICE_SN}=    Setup    register=${False}

    Execute Command    tedge config set device.id ${DEVICE_SN}
    Set Cumulocity URLs
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'

    Execute Command
    ...    cmd=printf '[c8y]\nusername = "%s"\npassword = "%s"\n' '${C8Y_CONFIG.tenant}/${C8Y_CONFIG.username}' '${C8Y_CONFIG.password}' > /etc/tedge/credentials.toml
    Execute Command    tedge config set c8y.auth_method basic

    Execute Command    tedge connect c8y

    Verify Custom Topic Publish

Connect to Cumulocity MQTT Service endpoint builtin bridge
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    Verify Custom Topic Publish


*** Keywords ***
Verify Custom Topic Publish
    [Documentation]    FUTURE: Deploy a lightweight service/function to be able to
    ...    test the messages published to and messages received from the Cumulocity
    ...    MQTT Service. For now, just test if the bridge is still up after publishing
    ...    a message to the mqtt service to ensure the bridge isn't being disconnected
    ...    due to publishing an illegal message or to an illegal topic
    # Publish a message to a topic that the device is subscribed to
    Execute Command    tedge mqtt pub c8y/mqtt/out/foo/bar '"hello"'
    # Assert that the message is looped back on the inbound subscribed topic
    Sleep    2s
    Bridge Should Be Up    cloud=c8y
