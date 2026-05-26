*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:c8y    theme:registration    theme:deregistration


*** Variables ***
${DEVICE_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}
${CHILD_XID}    ${EMPTY}


*** Test Cases ***
Twin fragment in registration message survives agent restart
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","name":"${CHILD_SN}-custom"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-custom

    # Re-publish registration with a different name — must NOT clobber the user's value
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-custom
    Device Should Have Fragment Values    name\=${CHILD_SN}-custom

Twin fragment set via twin topic survives agent restart
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${DEVICE_SN}:device:${CHILD_SN}

    Execute Command
    ...    tedge mqtt pub --retain "te/device/${CHILD_SN}///twin/name" '"${CHILD_SN}-custom"'
    Device Should Have Fragment Values    name\=${CHILD_SN}-custom

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-custom
    Device Should Have Fragment Values    name\=${CHILD_SN}-custom

Twin fragment in registration message updated via twin topic survives agent restart
    Skip
    ...    msg=When the registration message is re-delivered to the agent on startup, the twin value in it is accepted as the latest twin value, which causes the updated twin value to be lost.

    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    Execute Command
    ...    tedge mqtt pub --retain "te/device/${CHILD_SN}///twin/name" '"${CHILD_SN}-updated"'
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-updated
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

Twin fragment in registration message updated via re-registration survives agent restart
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    # Re-publish registration with a different name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-updated"}'
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-updated
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

Twin fragment update via re-registration message takes precedence over twin topic update on agent restart
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    Stop Service    tedge-agent

    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-updated"}'
    Execute Command
    ...    tedge mqtt pub --retain "te/device/${CHILD_SN}///twin/name" '"${CHILD_SN}-custom"'

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-updated
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

Twin fragment updated via twin topic while agent is offline survives agent restart
    Skip
    ...    msg=When the registration message is re-delivered to the agent on startup, the twin value in it is accepted as the latest twin value, which causes the updated twin value to be lost.

    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    Stop Service    tedge-agent

    Execute Command
    ...    tedge mqtt pub --retain "te/device/${CHILD_SN}///twin/name" '"${CHILD_SN}-updated"'

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-updated
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

Twin fragment updated via re-registration while agent is offline survives agent restart
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    Stop Service    tedge-agent

    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-updated"}'

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Have Retained MQTT Messages
    ...    te/device/${CHILD_SN}///twin/name
    ...    message_contains=${CHILD_SN}-updated
    Device Should Have Fragment Values    name\=${CHILD_SN}-updated

Twin fragment in registration message survives agent restart even after deletion
    Skip
    ...    Since clearing the twin message does not clear the twin value in the registration message, when it is re-delivered to the agent on startup, the twin value in it is accepted as the latest twin value.

    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_XID}","name":"${CHILD_SN}-initial"}'
    Device Should Exist    ${CHILD_XID}
    Device Should Have Fragment Values    name\=${CHILD_SN}-initial

    # Delete the twin/name fragment via empty retained payload
    Execute Command    tedge mqtt pub --retain "te/device/${CHILD_SN}///twin/name" ''

    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    Should Not Have Retained MQTT Messages    te/device/${CHILD_SN}///twin/name


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup
    ${child_sn}=    Get Random Name
    VAR    ${DEVICE_SN}=    ${device_sn}    scope=TEST
    VAR    ${CHILD_SN}=    ${child_sn}    scope=TEST
    VAR    ${CHILD_XID}=    ${device_sn}:device:${child_sn}    scope=TEST

    ThinEdgeIO.Set Device Context    ${DEVICE_SN}
