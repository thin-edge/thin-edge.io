*** Settings ***
Resource        ../../resources/common.resource
Library         ThinEdgeIO
Library         OperatingSystem
Library         String

Test Setup     Setup    skip_bootstrap=True
Suite Teardown  Get Logs

Test Tags      theme:mqtt


*** Test Cases ***

Publish Connection Closed after publish
    [Documentation]    Tests that the connection to the MQTT broker is closed after publishing.
    Execute Command    /setup/bootstrap.sh
    Execute Command    tedge mqtt pub test/topic Hello
    ${MQTT_PUB}    Execute Command    journalctl -u mosquitto -n 30
    Should Contain    ${MQTT_PUB}    Received DISCONNECT from tedge-pub

Publish Connection Closed after publish and no error message with TLS
    [Documentation]    same as previous test but with TLS
    Execute Command    /setup/bootstrap.sh --no-connect --no-secure
    Set up broker with server and client authentication
    tedge configure MQTT server authentication
    tedge configure MQTT client authentication
    Execute Command    tedge mqtt pub test/topic Hello
    ${MQTT_PUB}    Execute Command    journalctl -u mosquitto -n 30
    Should Contain    ${MQTT_PUB}    Received DISCONNECT from tedge-pub
    Should Not Contain    ${MQTT_PUB}    Error

Subscribe Connection Closed On Interruption
    [Documentation]    Tests that a subscribe connection to MQTT broker closes upon interruption.
    Execute Command    /setup/bootstrap.sh
    Execute Command    timeout 2 tedge mqtt sub test/topic    ignore_exit_code=True
    ${MQTT_SUB}    Execute Command    journalctl -u mosquitto -n 30
    Should Contain    ${MQTT_SUB}    Received DISCONNECT from tedge-sub

Stop subscription on SIGINT even when broker is not available
    Execute Command    /setup/bootstrap.sh
    Execute Command    sudo systemctl stop mosquitto
    ${output}=     Execute Command    cmd=timeout --signal=SIGINT 2 tedge mqtt sub '#'    timeout=5    stderr=${True}    stdout=${False}    ignore_exit_code=${True}
    Should Contain    ${output}    Connection refused



*** Keywords ***

Set up broker with server and client authentication
    Transfer To Device    ${CURDIR}/mosquitto-client-auth.conf    /etc/tedge/mosquitto-conf/mosquitto-client-auth.conf
    Execute Command
    ...    mv /etc/tedge/mosquitto-conf/mosquitto-client-auth.conf /etc/tedge/mosquitto-conf/tedge-mosquitto.conf
    Transfer To Device    ${CURDIR}/gen_certs.sh    /setup/gen_certs.sh
    Execute Command    chmod u+x /setup/gen_certs.sh
    Execute Command    /setup/gen_certs.sh
    Restart Service    mosquitto

tedge configure MQTT server authentication
    Execute Command    tedge config set mqtt.client.port 8883
    Execute Command    tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt

tedge configure MQTT client authentication
    Execute Command    tedge config set mqtt.client.auth.cert_file client.crt
    Execute Command    tedge config set mqtt.client.auth.key_file client.key
