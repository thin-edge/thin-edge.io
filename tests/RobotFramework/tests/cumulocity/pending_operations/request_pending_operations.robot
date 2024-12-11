*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation


*** Test Cases ***
Process any pending operations after connection disruptions to mosquitto bridge
    ThinEdgeIO.Bridge Should Be Up    c8y

    ThinEdgeIO.Disconnect From Network
    ThinEdgeIO.Execute Command    tedge connect c8y --test    exp_exit_code=1

    # Create cloud operation whilst the device is disconnected
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin

    # Restore connection
    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Bridge Should Be Up    c8y

    Operation Should Be SUCCESSFUL    ${operation}

Process any pending operations after connection disruptions to mosquitto bridge with custom topic-prefix
    ThinEdgeIO.Execute Command    tedge config set c8y.bridge.topic_prefix c8y-test
    ThinEdgeIO.Execute Command    tedge reconnect c8y
    ThinEdgeIO.Bridge Should Be Up    c8y-test

    ThinEdgeIO.Disconnect From Network
    # Verify we are definitely disconnected from cloud
    ThinEdgeIO.Execute Command    tedge connect c8y --profile test --test    exp_exit_code=1

    # Create cloud operation whilst the device is disconnected
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin

    # Restore connection
    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Bridge Should Be Up    c8y-test

    Operation Should Be SUCCESSFUL    ${operation}

Process any pending operations after connection disruptions to built_in bridge
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.built_in true
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.reconnect_policy.initial_interval 0
    ThinEdgeIO.Execute Command    tedge reconnect c8y
    ThinEdgeIO.Bridge Should Be Up    c8y

    ThinEdgeIO.Disconnect From Network
    # Verify we are definitely disconnected from cloud
    ThinEdgeIO.Execute Command    tedge connect c8y --test    exp_exit_code=1

    # Create cloud operation whilst the device is disconnected
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin

    # Restore connection
    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Bridge Should Be Up    c8y

    Operation Should Be SUCCESSFUL    ${operation}

Process any pending operations after connection disruptions to profiled built-in bridge
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.built_in true
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.reconnect_policy.initial_interval 0
    ThinEdgeIO.Execute Command    tedge config set c8y@test.url "$(tedge config get c8y.url)"
    ThinEdgeIO.Execute Command    tedge config set c8y@test.bridge.topic_prefix c8y-test
    ThinEdgeIO.Execute Command    tedge config unset c8y.url
    ThinEdgeIO.Execute Command    tedge disconnect c8y
    ThinEdgeIO.Execute Command    tedge connect c8y --profile test
    ThinEdgeIO.Bridge Should Be Up    c8y-test

    ThinEdgeIO.Disconnect From Network
    # Verify we are definitely disconnected from cloud
    ThinEdgeIO.Execute Command    tedge connect c8y --profile test --test    exp_exit_code=1

    # Create cloud operation whilst the device is disconnected
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin

    # Restore connection
    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Bridge Should Be Up    c8y-test

    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
