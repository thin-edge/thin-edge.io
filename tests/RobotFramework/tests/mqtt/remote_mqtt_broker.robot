*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:mqtt    theme:c8y    adapter:docker


*** Variables ***
${CONTAINER_1}      ${EMPTY}
${CONTAINER_2}      ${EMPTY}


*** Test Cases ***
Check remote mqtt broker #1773
    [Documentation]    The test relies on two containers functioning as one logical device. 1 container (CONTAINER_1)
    ...    runs the mqtt broker and the mapper, and the other container (CONTAINER_2) runs the other tedge components.
    ...    Together the two containers should function similar to installing everything in one container (more or less).
    ...    This is the building blocks for enabling a single-process container setup where each thin-edge.io component
    ...    is running in its own container.
    [Tags]    \#1773
    ThinEdgeIO.Set Device Context    ${CONTAINER_1}
    ThinEdgeIO.Service Should Be Running    mosquitto
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Stopped    tedge-agent
    ThinEdgeIO.Service Should Be Stopped    c8y-firmware-plugin

    ThinEdgeIO.Set Device Context    ${CONTAINER_2}
    ThinEdgeIO.Service Should Be Stopped    mosquitto
    ThinEdgeIO.Service Should Be Stopped    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-agent
    ThinEdgeIO.Service Should Be Running    c8y-firmware-plugin

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


*** Keywords ***
Custom Setup
    # Container 1 running mqtt host and mapper
    ${CONTAINER_1}=    Setup    bootstrap_args=--no-secure    register=${True}    connect=${False}
    Set Test Variable    $CONTAINER_1
    ${CONTAINER_1_IP}=    Get IP Address
    Disconnect Mapper    c8y
    Execute Command    sudo tedge config set mqtt.client.host ${CONTAINER_1_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.bind.address ${CONTAINER_1_IP}
    Connect Mapper    c8y
    Restart Service    mqtt-logger

    Stop Service    tedge-agent
    Stop Service    c8y-firmware-plugin

    # Copy files form one device to another (use base64 encoding to prevent quoting issues)
    ${tedge_toml_encoded}=    Execute Command    cat /etc/tedge/tedge.toml | base64
    ${pem}=    Execute Command    cat "$(tedge config get device.cert_path)"

    # container 2 running all other services
    ${CONTAINER_2}=    Setup    bootstrap_args=--no-secure    register=${False}
    Set Test Variable    $CONTAINER_2

    # Stop services that don't need to be running on the second device
    Stop Service    mosquitto
    Stop Service    tedge-mapper-c8y

    Execute Command    echo -n "${tedge_toml_encoded}" | base64 --decode | sudo tee /etc/tedge/tedge.toml
    Execute Command    sudo tedge config unset mqtt.bind.address
    Execute Command    echo "${pem}" | sudo tee "$(tedge config get device.cert_path)"
    Restart Service    c8y-firmware-plugin
    Restart Service    tedge-agent

Custom Teardown
    Get Logs    name=${CONTAINER_1}
    Get Logs    name=${CONTAINER_2}
