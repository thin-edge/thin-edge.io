*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup          Test Setup
Test Teardown       Get Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y


*** Test Cases ***
Restart agent on connection
    [Documentation]    tedge connect/reconnect should restart tedge-agent service to ensure it has the latest settings

    Service Should Be Running    tedge-agent
    ${agent_pid_before}    Execute Command    sudo systemctl show --property MainPID tedge-agent

    # Change some configuration
    Execute Command
    ...    tedge config unset mqtt.client.auth.ca_file && tedge config unset mqtt.client.auth.cert_file && tedge config unset mqtt.client.auth.key_file
    Execute Command    tedge config set mqtt.bind.port 1884 && tedge config set mqtt.client.port 1884

    Execute Command    sudo tedge reconnect c8y
    Execute Command    sudo tedge connect c8y --test
    ${agent_pid_after}    Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Not Be Equal    ${agent_pid_before}    ${agent_pid_after}

    Service Should Be Running    tedge-agent
    Service Health Status Should Be Up    tedge-agent


*** Keywords ***
Test Setup
    Setup
