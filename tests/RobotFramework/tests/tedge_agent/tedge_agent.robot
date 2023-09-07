*** Comments ***
#PRECONDITION:
#Device CH_DEV_CONF_MGMT is existing on tenant, if not
#use -v DeviceID:xxxxxxxxxxx in the command line to use your existing device


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             JSONLibrary
Library             Collections

Suite Setup         Custom Setup
Suite Teardown      Get Logs    name=${PARENT_SN}

Force Tags          theme:tedge_agent


*** Variables ***
${PARENT_IP}
${PARENT_SN}
${CHILD_SN}


*** Test Cases ***

tedge-agent starts on child device
    Set Device Context    ${CHILD_SN}
    Execute Command    dpkg -i packages/tedge_*.deb
    Execute Command    dpkg -i packages/tedge-agent_*.deb
    Start Service      tedge-agent
    # delay for tedge-agent to connect to parent device MQTT broker
    Sleep    3s
    Set Device Context    ${PARENT_SN}
    # we check that tedge-agent on child device was able to connect
    Execute Command    grep -q "Sending CONNACK to tedge-agent#te/device/child1//" /var/log/mosquitto/mosquitto.log


*** Keywords ***

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP} 
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    # Child
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
    Set Device Context    ${CHILD_SN}
    Execute Command       echo '[mqtt]' >> /etc/tedge/tedge.toml
    Execute Command       echo 'device_topic_id \= "device/child1//"' >> /etc/tedge/tedge.toml
    Execute Command       echo 'client.host \= "${PARENT_IP}"' >> /etc/tedge/tedge.toml
