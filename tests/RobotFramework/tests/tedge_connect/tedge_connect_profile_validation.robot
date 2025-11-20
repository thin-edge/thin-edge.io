*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:mqtt    theme:c8y    adapter:docker


*** Test Cases ***
Bridge topic prefix must be unique to connect
    ThinEdgeIO.Execute Command    sudo tedge config set c8y.url --profile second example.com    timeout=0
    Verify conflicting configuration error appears    sudo tedge connect c8y --profile second    bridge.topic_prefix

Bridge topic prefix must be unique to test connection
    ThinEdgeIO.Execute Command    sudo tedge config set c8y.url --profile second example.com    timeout=0
    Verify conflicting configuration error appears
    ...    sudo tedge connect c8y --test --profile second
    ...    bridge.topic_prefix

Device ID must be unique if cloud URLs are the same
    ThinEdgeIO.Execute Command
    ...    sudo tedge config set c8y.mqtt --profile second "$(tedge config get c8y.mqtt)"
    ...    timeout=0
    ThinEdgeIO.Execute Command
    ...    sudo tedge config set c8y.http --profile second "$(tedge config get c8y.http)"
    ...    timeout=0
    # Just assert the command fails, we have unit tests that assert the correct error message appears
    ThinEdgeIO.Execute Command
    ...    sudo tedge connect c8y --test --profile second
    ...    exp_exit_code=1
    ...    timeout=0

Proxy bind port must be unique
    ThinEdgeIO.Execute Command    sudo tedge config set c8y.url --profile second example.com    timeout=0
    ThinEdgeIO.Execute Command
    ...    sudo tedge config set c8y.bridge.topic_prefix --profile second c8y-second
    ...    timeout=0
    Verify conflicting configuration error appears    sudo tedge connect c8y --profile second    proxy.bind.port


*** Keywords ***
Verify conflicting configuration error appears
    [Arguments]    ${command}    ${conflicting_configuration}
    ${output}=    ThinEdgeIO.Execute Command
    ...    ${command}
    ...    exp_exit_code=1
    ...    stdout=False
    ...    stderr=True
    ...    timeout=0
    Should Contain
    ...    ${output}
    ...    Error
    Should Contain
    ...    ${output}
    ...    The configurations: c8y.${conflicting_configuration}, c8y.profiles.second.${conflicting_configuration} should be set to different values before connecting, but are currently set to the same value\n
