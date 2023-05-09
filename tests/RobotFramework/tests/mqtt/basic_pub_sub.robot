*** Settings ***
Resource        ../../resources/common.resource
Library         ThinEdgeIO
Library         OperatingSystem

Suite Setup     Custom Setup

Force Tags      theme:mqtt


*** Test Cases ***
Publish on a local insecure broker
    Start Service    mosquitto
    Execute Command    tedge mqtt pub topic message

Publish on a local secure broker
    Set up broker server authentication
    Restart Service    mosquitto
    tedge configure MQTT server authentication
    Execute Command    tedge mqtt pub topic message

Publish on a local secure broker with client authentication
    Set up broker with server and client authentication
    Restart Service    mosquitto
    tedge configure MQTT server authentication
    tedge configure MQTT client authentication
    Execute Command    tedge mqtt pub topic message


*** Keywords ***
Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    /setup/bootstrap.sh --no-connect --no-secure

Set up broker server authentication
    Transfer To Device    ${CURDIR}/mosquitto-server-auth.conf    /etc/tedge/mosquitto-conf/
    Execute Command
    ...    mv /etc/tedge/mosquitto-conf/mosquitto-server-auth.conf /etc/tedge/mosquitto-conf/tedge-mosquitto.conf
    Transfer To Device    ${CURDIR}/gen_certs.sh    /root/gen_certs.sh
    Execute Command    chmod u+x /root/gen_certs.sh
    Execute Command    /root/gen_certs.sh

Set up broker with server and client authentication
    Transfer To Device    ${CURDIR}/mosquitto-client-auth.conf    /etc/tedge/mosquitto-conf/mosquitto-client-auth.conf
    Execute Command
    ...    mv /etc/tedge/mosquitto-conf/mosquitto-client-auth.conf /etc/tedge/mosquitto-conf/tedge-mosquitto.conf
    Transfer To Device    ${CURDIR}/gen_certs.sh    /root/gen_certs.sh
    Execute Command    chmod u+x /root/gen_certs.sh
    Execute Command    /root/gen_certs.sh

tedge configure MQTT server authentication
    Execute Command    tedge config set mqtt.client.port 8883
    Execute Command    tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt

tedge configure MQTT client authentication
    Execute Command    tedge config set mqtt.client.auth.cert_file client.crt
    Execute Command    tedge config set mqtt.client.auth.key_file client.key
