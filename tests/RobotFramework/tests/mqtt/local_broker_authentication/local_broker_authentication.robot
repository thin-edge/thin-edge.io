*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Teardown       Custom Teardown

Test Tags           theme:mqtt    theme:c8y    adapter:docker


*** Variables ***
${CONTAINER_1}      ${EMPTY}
${CONTAINER_2}      ${EMPTY}


*** Test Cases ***
Username/password authentication to local broker without TLS - mosquitto bridge
    [Setup]    No TLS Setup    use_builtin_bridge=false
    Check system health including c8y

Username/password authentication to local broker without TLS - built-in bridge
    [Setup]    No TLS Setup    use_builtin_bridge=true
    Check system health including c8y

Username/password authentication to local broker with TLS - mosquitto bridge
    [Setup]    TLS Setup    use_builtin_bridge=false
    Check system health including c8y

Username/password authentication to local broker with TLS - built-in bridge
    [Setup]    TLS Setup    use_builtin_bridge=true
    Check system health including c8y


*** Keywords ***
Check system health including c8y
    ThinEdgeIO.Set Device Context    ${CONTAINER_1}
    ThinEdgeIO.Service Should Be Running    mosquitto
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-agent
    ThinEdgeIO.Service Should Be Running    c8y-firmware-plugin

    ThinEdgeIO.Set Device Context    ${CONTAINER_2}
    ThinEdgeIO.Service Should Be Stopped    mosquitto
    ThinEdgeIO.Service Should Be Stopped    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-agent
    ThinEdgeIO.Service Should Be Stopped    c8y-firmware-plugin

    # tedge mqtt pub/sub sanity check
    ${output}=    Execute Command    tedge mqtt pub test-topic test-message    stderr=${True}
    Should Not Contain    ${output}    Failed to connect to broker
    ${output}=    Execute Command    tedge mqtt sub test --duration 1s    stderr=${True}
    Should Not Contain    ${output}    Failed to connect to broker

    # Validate the device exists in the cloud
    Cumulocity.Device Should Exist    ${CONTAINER_1}

    # Cumulocity sanity check
    ThinEdgeIO.Set Device Context    ${CONTAINER_2}
    ThinEdgeIO.Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 29.8}'
    ${measurements}=    Cumulocity.Device Should Have Measurements    value=temperature    series=temperature
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    ${29.8}

    Cumulocity.Should Have Services    name=tedge-mapper-c8y    status=up
    Cumulocity.Should Have Services    name=tedge-agent    status=up
    Cumulocity.Should Have Services    name=c8y-firmware-plugin    status=up

No TLS Setup
    [Arguments]    ${use_builtin_bridge}

    # Parent
    ${CONTAINER_1}=    Setup    bootstrap_args=--no-secure    register=${True}    connect=${False}
    Set Test Variable    $CONTAINER_1
    ${CONTAINER_1_HOSTNAME}=    Execute Command    hostname    strip=${True}

    Execute Command    sudo tedge config set mqtt.bridge.built_in ${use_builtin_bridge}
    Execute Command    mosquitto_passwd -c -b /etc/mosquitto/pwfile testuser testpassword

    Transfer To Device    ${CURDIR}/unencrypted-listener.conf    /etc/mosquitto/conf.d/
    Configure MQTT client    host=127.0.0.1    port=1884

    Connect Mapper    c8y

    # Child
    ${CONTAINER_2}=    Setup    bootstrap_args=--no-secure    register=${False}    connect=${False}
    Set Test Variable    $CONTAINER_2

    Configure MQTT client    host=${CONTAINER_1_HOSTNAME}    port=1884
    Start Service    tedge-agent

TLS Setup
    [Arguments]    ${use_builtin_bridge}

    # Parent
    ${CONTAINER_1}=    Setup    bootstrap_args=--secure    register=${True}    connect=${False}
    Set Test Variable    $CONTAINER_1
    ${CONTAINER_1_HOSTNAME}=    Execute Command    hostname    strip=${True}

    Execute Command    sudo tedge config set mqtt.bridge.built_in ${use_builtin_bridge}
    Execute Command    mosquitto_passwd -c -b /etc/mosquitto/pwfile testuser testpassword

    Transfer To Device    ${CURDIR}/encrypted-listener.conf    /etc/mosquitto/conf.d/
    Configure MQTT client    host=127.0.0.1    port=8884
    # Remove the certificate based authentication settings
    Execute Command    sudo tedge config unset mqtt.client.auth.cert_file
    Execute Command    sudo tedge config unset mqtt.client.auth.key_file

    # Copy CA from the parent to child
    ${ca}=    Execute Command    cat "$(tedge config get mqtt.client.auth.ca_file)"

    Connect Mapper    c8y

    # Child
    ${CONTAINER_2}=    Setup    bootstrap_args=--no-secure    register=${False}    connect=${False}
    Set Test Variable    $CONTAINER_2

    Execute Command    tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt
    Execute Command    echo "${ca}" | sudo tee "$(tedge config get mqtt.client.auth.ca_file)"
    Configure MQTT client    host=${CONTAINER_1_HOSTNAME}    port=8884
    Start Service    tedge-agent

Configure MQTT client
    [Arguments]    ${host}    ${port}
    Execute Command    echo testpassword > /etc/tedge/.password
    Execute Command    sudo tedge config set mqtt.client.auth.username testuser
    Execute Command    sudo tedge config set mqtt.client.auth.password_file /etc/tedge/.password
    Execute Command    sudo tedge config set mqtt.client.host ${host}
    Execute Command    sudo tedge config set mqtt.client.port ${port}

Custom Teardown
    Get Logs    name=${CONTAINER_1}
    Get Logs    name=${CONTAINER_2}
