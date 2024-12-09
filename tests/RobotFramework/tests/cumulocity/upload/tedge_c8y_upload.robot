*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y


*** Variables ***
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


*** Test Cases ***
Upload a file to Cumulocity from main device
    ThinEdgeIO.Set Device Context    ${PARENT_SN}
    Execute Command    yes 0123456789 | head>/tmp/sample.txt
    ${event_id}=    Execute Command
    ...    tedge upload c8y --file /tmp/sample.txt --mime-type text/plain --type "test event" --text "testing file upload"
    ${events}=    Device Should Have Event/s
    ...    type=test event
    ...    expected_text=testing file upload
    ...    with_attachment=True
    ...    minimum=1
    ...    maximum=1
    Should Be Equal    "${events[0]["id"]}\n"    "${event_id}"
    Should Be Equal    ${events[0]["c8y_IsBinary"]["name"]}    sample.txt
    Should Be Equal    ${events[0]["c8y_IsBinary"]["type"]}    text/plain

Upload a file to Cumulocity from child device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Cumulocity.Set Managed Object    ${CHILD_SN}
    Execute Command    yes child | head>/tmp/sample.txt
    ${event_id}=    Execute Command
    ...    tedge upload c8y --file /tmp/sample.txt
    ${events}=    Device Should Have Event/s
    ...    type=tedge_UploadedFile
    ...    expected_text=Uploaded file: "/tmp/sample.txt"
    ...    with_attachment=True
    ...    minimum=1
    ...    maximum=1
    Log    ${events}
    Should Be Equal    "${events[0]["id"]}\n"    "${event_id}"
    Should Be Equal    ${events[0]["c8y_IsBinary"]["name"]}    sample.txt
    Should Be Equal    ${events[0]["c8y_IsBinary"]["type"]}    application/octet-stream


*** Keywords ***
Custom Setup
    ${parent_ip}=    Setup Main Device
    Setup Child Device    ${parent_ip}

Setup Main Device
    ${parent_sn}=    Setup    skip_bootstrap=${False}
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Execute Command    sudo tedge config set mqtt.external.bind.address ${parent_ip}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883
    Execute Command    sudo tedge config set c8y.proxy.bind.address ${parent_ip}
    Execute Command    sudo tedge config set c8y.proxy.client.host ${parent_ip}
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
    RETURN    ${parent_ip}

Setup Child Device
    [Arguments]    ${parent_ip}
    ${child_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN    ${child_sn}

    Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb
    Execute Command    sudo tedge config set mqtt.client.host ${parent_ip}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set c8y.proxy.client.host ${parent_ip}
    Execute Command    sudo tedge config set mqtt.device_topic_id device/${child_sn}//

    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_SN}"}'
    RETURN    ${child_sn}
