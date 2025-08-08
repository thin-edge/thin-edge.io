*** Settings ***
Library             ThinEdgeIO

Suite Setup          Custom Setup
Suite Teardown       Get Logs

Test Tags           theme:tedge_flows


*** Test Cases ***
Add missing timestamps
    ${transformed_msg}    Execute Command    tedge flows test te/device/main///m/ '{}'
    Should Contain    ${transformed_msg}    item=time

Convert timestamps to ISO
    ${transformed_msg}    Execute Command    tedge flows test te/device/main///m/ '{"time": 1751023862.000}'
    Should Contain    ${transformed_msg}    item="time":"2025-06-27T11:31:02.000Z"

Extract measurement type from topic
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"environment"

Use default measurement type
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/ '{"temperature": 258}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="type":"ThinEdgeMeasurement"

Translate complex tedge json to c8y json
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"time":"2025-06-27T08:11:05.301804125Z", "temperature": 258, "location": {"latitude": 32.54, "longitude": -117.67, "altitude": 98.6 }, "pressure": 98}'
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    [c8y/measurement/measurements/create] {"type":"environment","time":"2025-06-27T08:11:05.301804125Z","temperature":{"temperature":258},"location":{"latitude":32.54,"longitude":-117.67,"altitude":98.6},"pressure":{"pressure":98}}

Units are configured using topic metadata
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/measurements.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/measurements.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Computing average over a time window
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/average.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval --flow /etc/tedge/flows/average.js
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/average.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Each instance of a script must have its own static state
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/flows/count-messages.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --final-on-interval | sort
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/flows/count-messages.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: ' | sort
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup    connect=${False}
    Set Suite Variable    $DEVICE_SN
    Copy Configuration Files

Copy Configuration Files
    Execute Command    mkdir /etc/tedge/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/*    /etc/tedge/flows/
