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

    Verify Custom Topic Publish and Subscribe

Connect to Cumulocity MQTT Service endpoint basic auth
    Skip    msg=Until this issue is fixed on eu-latest

    ${DEVICE_SN}=    Setup    register=${False}

    Execute Command    tedge config set device.id ${DEVICE_SN}
    Set Cumulocity URLs
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'

    Execute Command
    ...    cmd=printf '[c8y]\nusername = "%s"\npassword = "%s"\n' '${C8Y_CONFIG.tenant}/${C8Y_CONFIG.username}' '${C8Y_CONFIG.password}' > /etc/tedge/credentials.toml
    Execute Command    tedge config set c8y.auth_method basic

    Execute Command    tedge connect c8y

    Verify Custom Topic Publish and Subscribe

Connect to Cumulocity MQTT Service endpoint builtin bridge
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set c8y.mqtt_service.enabled true
    Execute Command    tedge config set c8y.mqtt_service.topics 'sub/topic,demo/topic'
    Execute Command    tedge connect c8y

    Verify Custom Topic Publish and Subscribe


*** Keywords ***
Verify Custom Topic Publish and Subscribe
    ${timestamp}=    Get Unix Timestamp
    # Publish a message to a topic that the device is subscribed to
    Execute Command    tedge mqtt pub c8y/mqtt/out/sub/topic '"hello"'
    # Assert that the message is looped back on the inbound subscribed topic
    Should Have MQTT Messages
    ...    c8y/mqtt/in/sub/topic
    ...    message_contains="hello"
    ...    date_from=${timestamp}
