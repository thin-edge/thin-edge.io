*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Successful log operation
    Cumulocity.Should Contain Supported Operations    c8y_LogfileRequest
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log File Contents Should Be Equal    ${operation}    filename: example.log\n1 first line\n


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb packages/tedge-agent*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    Execute Command
    ...    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log && touch /var/log/example/example.log

    Enable Service    tedge-agent
    Start Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $PARENT_SN    ${parent_sn}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set c8y.enable.log_upload true
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883
    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    ${CHILD_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN
    Set Suite Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_XID}

Custom Teardown
    Get Logs    ${PARENT_SN}
    Get Logs    ${CHILD_SN}

Log File Contents Should Be Equal
    [Arguments]
    ...    ${operation}
    ...    ${expected_contents}
    ...    ${encoding}=utf-8
    ...    ${expected_filename}=^${CHILD_XID}_[\\w\\W]+-c8y-mapper-\\d+$
    ...    ${expected_mime_type}=text/plain
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_contents=${expected_contents}
    ...    encoding=${encoding}
    ${event}=    Cumulocity.Event Attachment Should Have File Info
    ...    ${event_id}
    ...    name=${expected_filename}
    ...    mime_type=${expected_mime_type}
    RETURN    ${contents}
