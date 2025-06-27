*** Settings ***
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_mapper

*** Test Cases ***
Add missing timestamps
    ${transformed_msg}    Execute Command    tedge mapping test te/device/main///m/ '{}'
    Should Contain    ${transformed_msg}    item=time

Convert timestamps to ISO
    ${transformed_msg}    Execute Command    tedge mapping test te/device/main///m/ '{"time": 1751023862.000}'
    Should Contain    ${transformed_msg}    item="time":"2025-06-27T11:31:02.000Z"

Extract measurement type from topic
    ${transformed_msg}    Execute Command
    ...    tedge mapping test te/device/main///m/environment '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"environment"

Use default measurement type
    ${transformed_msg}    Execute Command
    ...    tedge mapping test te/device/main///m/ '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"ThinEdgeMeasurement"

Translate complex tedge json to c8y json
    ${transformed_msg}    Execute Command
    ...    tedge mapping test te/device/main///m/environment '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258, "location": {"latitude": 32.54, "longitude": -117.67, "altitude": 98.6 }, "pressure": 98}'
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] {"type":"environment","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":258},"location":{"latitude":32.54,"longitude":-117.67,"altitude":98.6},"pressure":{"pressure":98}}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup     skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Set Suite Variable    $DEVICE_SN
    Copy Configuration Files

Copy Configuration Files
    Execute Command    mkdir /etc/tedge/gen-mapper/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/pipelines/*    /etc/tedge/gen-mapper/

