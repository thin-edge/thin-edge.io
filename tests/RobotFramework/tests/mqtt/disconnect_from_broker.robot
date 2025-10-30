*** Settings ***
Resource            ../../resources/common.resource
Library             OperatingSystem
Library             String
Library             ThinEdgeIO

Suite Teardown      Get Suite Logs
Test Timeout        5 minutes

Test Tags           theme:mqtt


*** Test Cases ***
Publish Connection Closed after publish
    [Documentation]    Tests that the connection to the MQTT broker is closed after publishing.
    [Setup]    Setup
    ${log_start}    ThinEdgeIO.Get Unix Timestamp
    Execute Command    tedge mqtt pub test/topic Hello
    ${MQTT_PUB}    Execute Command    journalctl -u mosquitto --since "@${log_start}" --no-pager
    Should Contain    ${MQTT_PUB}    Received DISCONNECT from tedge-pub

Publish Connection Closed after publish and no error message with TLS
    [Documentation]    same as previous test but with TLS
    [Setup]    Setup    bootstrap_args=--no-secure    register=${False}
    Set up broker with server and client authentication
    tedge configure MQTT server authentication
    tedge configure MQTT client authentication
    ${log_start}    ThinEdgeIO.Get Unix Timestamp
    Execute Command    tedge mqtt pub test/topic Hello
    ${MQTT_PUB}    Execute Command    journalctl -u mosquitto --since "@${log_start}" --no-pager
    Should Contain    ${MQTT_PUB}    Received DISCONNECT from tedge-pub

Subscribe Connection Closed On Interruption
    [Documentation]    Tests that a subscribe connection to MQTT broker closes upon interruption.
    [Setup]    Setup
    ${log_start}    ThinEdgeIO.Get Unix Timestamp
    Execute Command    timeout 2 tedge mqtt sub test/topic    ignore_exit_code=True
    ${MQTT_SUB}    Execute Command    journalctl -u mosquitto --since "@${log_start}" --no-pager
    Should Contain    ${MQTT_SUB}    Received DISCONNECT from tedge-sub

Stop subscription on SIGINT even when broker is not available
    [Setup]    Setup
    Execute Command    sudo systemctl stop mosquitto
    ${output}    Execute Command
    ...    cmd=timeout --signal=SIGINT 2 tedge mqtt sub '#'
    ...    timeout=5
    ...    stderr=${True}
    ...    stdout=${False}
    ...    ignore_exit_code=${True}
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
    Execute Command    tedge config set mqtt.client.auth.cert_file "$(pwd)/client.crt"
    Execute Command    tedge config set mqtt.client.auth.key_file "$(pwd)/client.key"
