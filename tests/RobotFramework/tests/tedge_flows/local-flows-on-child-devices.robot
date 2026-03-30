*** Settings ***
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:tedge_flows


*** Test Cases ***
Child devices can run their own tedge-mapper-local
    Set Device Context    ${PARENT_SN}
    Execute Command    mkdir -p /etc/tedge/mappers/local/flows
    Execute Command    rm -fr /etc/tedge/mappers/local/flows/*
    Restart Service    tedge-mapper-local

    Set Device Context    ${CHILD_SN}
    Execute Command    mkdir -p /etc/tedge/mappers/local/flows
    ThinEdgeIO.Transfer To Device    ${CURDIR}/greeting-flows/*    /etc/tedge/mappers/local/flows/greeting-flows/
    ${start}=    Get Unix Timestamp
    Restart Service    tedge-mapper-local
    Service Should Be Running    tedge-mapper-local

    Set Device Context    ${PARENT_SN}
    Service Should Be Running    tedge-mapper-local
    Should Have MQTT Messages
    ...    topic=te/device/${CHILD_SN}/service/tedge-mapper-local/status/flows
    ...    message_contains=greeting-flows
    ...    date_from=${start}


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

    Execute Command    sudo dpkg -i packages/tedge_*.deb
    Execute Command    sudo dpkg -i packages/tedge-mapper_*.deb
    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set http.client.port 8000
    Execute Command    sudo tedge config set c8y.proxy.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set c8y.proxy.client.port 8001
    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

Custom Teardown
    Get Logs    name=${PARENT_SN}
    Get Logs    name=${CHILD_SN}
