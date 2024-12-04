*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y

*** Test Cases ***
Upload a file to Cumulocity
    Execute Command    yes 0123456789 | head>/tmp/sample.txt
    ${event_id}=   Execute Command
    ...    tedge c8y upload --file /tmp/sample.txt --mime-type text/plain --type "test event" --text "testing file upload"
    ${events}=    Device Should Have Event/s
    ...    type=test event
    ...    expected_text=testing file upload
    ...    with_attachment=True
    ...    minimum=1
    ...    maximum=1
    Should Be Equal    "${events[0]["id"]}\n"    "${event_id}"
    Should Be Equal    ${events[0]["c8y_IsBinary"]["name"]}    sample.txt
    Should Be Equal    ${events[0]["c8y_IsBinary"]["type"]}    text/plain

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y