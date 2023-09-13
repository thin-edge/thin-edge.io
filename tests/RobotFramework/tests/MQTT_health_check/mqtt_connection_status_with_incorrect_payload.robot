*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:troubleshooting    theme:monitoring    theme:c8y
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Send incorrect mqtt payload
    Execute Command    tedge mqtt pub 'tedge/commands/req/software/list' '{"ids": "100"}'
    ${output}=    Execute Command    systemctl status tedge-agent.service
    Should Not Contain    ${output}    INFO mqtt_channel::connection: MQTT connection closed

Send incorrect mqtt payload again
    Execute Command    tedge mqtt pub 'tedge/commands/req/software/list' '{"ids": "100"}'
    ${output}=    Execute Command    systemctl status tedge-agent.service
    Should Not Contain    ${output}    INFO mqtt_channel::connection: MQTT connection closed

Send incorrect mqtt payload third time
    Execute Command    tedge mqtt pub 'tedge/commands/req/software/list' '{"ids": "100"}'
    ${output}=    Execute Command    systemctl status tedge-agent.service
    Should Not Contain    ${output}    INFO mqtt_channel::connection: MQTT connection closed
