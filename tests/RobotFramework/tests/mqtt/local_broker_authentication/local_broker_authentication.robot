*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Teardown       Custom Teardown

Test Tags           theme:mqtt    theme:c8y    adapter:docker


*** Variables ***
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


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

Username/password + certificate authentication to local broker - mosquitto bridge
    [Setup]    TLS and Client Cert Setup    use_builtin_bridge=false
    Check system health including c8y

Username/password + certificate authentication to local broker - built-in bridge
    [Setup]    TLS and Client Cert Setup    use_builtin_bridge=true
    Check system health including c8y


*** Keywords ***
Check system health including c8y
    ThinEdgeIO.Set Device Context    ${PARENT_SN}
    ThinEdgeIO.Service Should Be Running    mosquitto
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-agent
    ThinEdgeIO.Service Should Be Running    c8y-firmware-plugin

    ThinEdgeIO.Set Device Context    ${CHILD_SN}
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
    Cumulocity.Device Should Exist    ${PARENT_SN}

    # Cumulocity sanity check
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    ThinEdgeIO.Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 29.8}'
    ${measurements}=    Cumulocity.Device Should Have Measurements    value=temperature    series=temperature
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    ${29.8}

    Cumulocity.Should Have Services    name=tedge-mapper-c8y    status=up
    Cumulocity.Should Have Services    name=tedge-agent    status=up
    Cumulocity.Should Have Services    name=c8y-firmware-plugin    status=up

No TLS Setup
    [Arguments]    ${use_builtin_bridge}

    # Parent
    ${PARENT_SN}=    Setup    bootstrap_args=--no-secure    register=${True}    connect=${False}
    Set Test Variable    $PARENT_SN
    ${PARENT_HOSTNAME}=    Execute Command    hostname    strip=${True}

    Execute Command    sudo tedge config set mqtt.bridge.built_in ${use_builtin_bridge}
    Execute Command    mosquitto_passwd -c -b /etc/mosquitto/pwfile testuser testpassword

    Transfer To Device    ${CURDIR}/unencrypted-listener.conf    /etc/mosquitto/conf.d/
    Configure MQTT client    host=127.0.0.1    port=1884

    Connect Mapper    c8y

    # Child
    ${CHILD_SN}=    Setup    bootstrap_args=--no-secure    register=${False}    connect=${False}
    Set Test Variable    $CHILD_SN

    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"
    Configure MQTT client    host=${PARENT_HOSTNAME}    port=1884
    Start Service    tedge-agent

TLS Setup
    [Arguments]    ${use_builtin_bridge}

    # Parent
    ${PARENT_SN}=    Setup    bootstrap_args=--secure    register=${True}    connect=${False}
    Set Test Variable    $PARENT_SN
    ${PARENT_HOSTNAME}=    Execute Command    hostname    strip=${True}

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
    ${CHILD_SN}=    Setup    bootstrap_args=--no-secure    register=${False}    connect=${False}
    Set Test Variable    $CHILD_SN

    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"
    Execute Command    sudo tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt
    Execute Command    echo "${ca}" | sudo tee "$(tedge config get mqtt.client.auth.ca_file)"
    Configure MQTT client    host=${PARENT_HOSTNAME}    port=8884
    Start Service    tedge-agent

TLS and Client Cert Setup
    [Arguments]    ${use_builtin_bridge}

    # Parent
    ${PARENT_SN}=    Setup    bootstrap_args=--secure    register=${True}    connect=${False}
    Set Test Variable    $PARENT_SN
    ${PARENT_HOSTNAME}=    Execute Command    hostname    strip=${True}

    Execute Command    sudo tedge config set mqtt.bridge.built_in ${use_builtin_bridge}
    Execute Command    mosquitto_passwd -c -b /etc/mosquitto/pwfile testuser testpassword

    Transfer To Device    ${CURDIR}/cert-pass-listener.conf    /etc/mosquitto/conf.d/
    Configure MQTT client    host=127.0.0.1    port=8885

    # Copy CA, certificate and key from the parent to child
    ${ca}=    Execute Command    cat "$(tedge config get mqtt.client.auth.ca_file)"
    ${cert}=    Execute Command    cat "$(tedge config get mqtt.client.auth.cert_file)"
    ${key}=    Execute Command    cat "$(tedge config get mqtt.client.auth.key_file)"

    Connect Mapper    c8y

    # Child
    ${CHILD_SN}=    Setup    bootstrap_args=--no-secure    register=${False}    connect=${False}
    Set Test Variable    $CHILD_SN

    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"
    Execute Command    sudo tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt
    Execute Command    sudo tedge config set mqtt.client.auth.cert_file /setup/client.crt
    Execute Command    sudo tedge config set mqtt.client.auth.key_file /setup/client.key
    Execute Command    echo "${ca}" | sudo tee "$(tedge config get mqtt.client.auth.ca_file)"
    Execute Command    echo "${cert}" | sudo tee "$(tedge config get mqtt.client.auth.cert_file)"
    Execute Command    echo "${key}" | sudo tee "$(tedge config get mqtt.client.auth.key_file)"

    Configure MQTT client    host=${PARENT_HOSTNAME}    port=8885
    Start Service    tedge-agent

Configure MQTT client
    [Arguments]    ${host}    ${port}
    Execute Command    echo testpassword > /etc/tedge/.password
    Execute Command    sudo tedge config set mqtt.client.auth.username testuser
    Execute Command    sudo tedge config set mqtt.client.auth.password_file /etc/tedge/.password
    Execute Command    sudo tedge config set mqtt.client.host ${host}
    Execute Command    sudo tedge config set mqtt.client.port ${port}

Custom Teardown
    Get Logs    name=${PARENT_SN}
    Get Logs    name=${CHILD_SN}
