*** Settings ***
Library    ThinEdgeIO


*** Variables ***
${FLOWS_DIR}    /etc/tedge/mappers/local/flows/
${TOPIC}        test/nested-flows/myflow


*** Keywords ***
Move Directory And Assert it's added and removed
    [Documentation]    Test that flow is automatically added/removed when flow directory is moved in/out of the flows dir.
    ...    Path src should be a directory with the flow inside.
    [Arguments]    ${src}    ${when}
    # moving the directory into flows dir should start the flow
    Execute Command    mv ${src} ${FLOWS_DIR}
    Assert flow is running    ${when}

    # now moving the directory again should stop the flow
    Execute Command    mv ${FLOWS_DIR}/myflow ${src}
    Assert flow is not running    ${when}

Assert flow is running
    [Arguments]    ${when}=
    ${start}    Get Unix Timestamp
    Execute Command    sleep 1
    Execute Command    tedge mqtt pub ${TOPIC} 'flow started ${when}'
    Logs Should Contain    JavaScript.Console: "flow started ${when}"    date_from=${start}    max_matches=1

Assert flow is not running
    [Arguments]    ${when}=
    ${start}    Get Unix Timestamp
    Execute Command    sleep 1
    Execute Command    tedge mqtt pub ${TOPIC} 'flow removed ${when}'
    Logs Should Not Contain    JavaScript.Console: "flow removed ${when}"    date_from=${start}
