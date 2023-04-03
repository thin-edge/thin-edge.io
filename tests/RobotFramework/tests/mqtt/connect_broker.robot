*** Settings ***
Resource        ../../resources/common.resource
Library         ThinEdgeIO

Suite Setup     Custom Setup

Force Tags      theme:mqtt


*** Test Cases ***
Publish on a local insecure broker
    Start Service      mosquitto
    Execute Command    tedge mqtt pub topic message

Publish on a local secure broker
    Set up broker server authentication
    Start Service      mosquitto
    Execute Command    tedge config set mqtt.client.host $(hostname)
    Execute Command    tedge config set mqtt.client.port 8883
    Execute Command    tedge config set mqtt.client.ca_file /etc/mosquitto/ca_certificates/ca.crt
    Execute Command    sleep 1
    Execute Command    tedge mqtt pub topic message


*** Keywords ***
Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    /setup/bootstrap.sh --no-connect

Set up broker server authentication
    Transfer To Device    ${CURDIR}/tedge-mosquitto.conf    /etc/tedge/mosquitto-conf/
    Transfer To Device    ${CURDIR}/gen_certs.sh    /root/gen_certs.sh
    Execute Command       chmod u+x /root/gen_certs.sh
    Execute Command       /root/gen_certs.sh
