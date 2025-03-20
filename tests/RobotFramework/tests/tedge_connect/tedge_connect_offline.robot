*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:mqtt    theme:c8y    adapter:docker


*** Test Cases ***
Check offline bootstrapping
    ThinEdgeIO.Execute Command    sudo tedge disconnect c8y
    ThinEdgeIO.Service Should Be Stopped    tedge-mapper-c8y

    ThinEdgeIO.Disconnect From Network
    ThinEdgeIO.Execute Command    sudo tedge connect c8y --offline
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ThinEdgeIO.Execute Command    sudo tedge connect c8y --test    exp_exit_code=!0

    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Execute Command    sudo tedge connect c8y --test
