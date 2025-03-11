*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:mqtt    theme:c8y


*** Test Cases ***
Agent Processes Operations After Local MQTT Broker Disconnects Without Mosquitto Persistence Settings
    # Force tedge-agent to disconnect by restarting mosquitto
    Restart Service    mosquitto
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    # Remove mosquitto persistence settings
    Execute Command    sed -i '/^persistence.*/d' /etc/mosquitto/mosquitto.conf

    # Restart mosquitto to start again ignoring pre-existing persisted data
    # Note: Tests should not rely on the mosquitto service restarting in the test setup
    Restart Service    mosquitto
