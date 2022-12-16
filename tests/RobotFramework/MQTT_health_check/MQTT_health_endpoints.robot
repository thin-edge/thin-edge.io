#Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/MQTT_health_check/MQTT_health_endpoints.robot


*** Settings ***
Library    SSHLibrary
Suite Setup            Open Connection And Log In
Suite Teardown         SSHLibrary.Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=
${pid}

*** Tasks ***
Connect to Cumulocity
    ${rc}=    Execute Command    sudo tedge connect c8y
    Log    ${rc}=

Start c8y-log-plugin
    ${rc}=    Execute Command    sudo systemctl start c8y-log-plugin.service    return_stdout=False    return_rc=True
    Should Be Equal As Integers    ${rc}    0

Note the PID Number
    ${pid}    Execute Command    pgrep c8y-log-plugin
    Set Suite Variable    ${pid}

Start MQTT Subscriber c8y-log-plugin & send empty message
   
    Write    sudo tedge mqtt sub 'tedge/health/c8y-log-plugin' &
    Write    sudo tedge mqtt pub 'tedge/health-check/c8y-log-plugin' ''
    ${output}=         Read    delay=1s
    Should Contain    ${output}    "pid":${pid}
    Should Contain    ${output}    "status":"up"

Start c8y-configuration-plugin
    ${rc}=    Execute Command    sudo systemctl start c8y-configuration-plugin.service    return_stdout=False    return_rc=True
    Should Be Equal As Integers    ${rc}    0

Note the PID Number
    ${pid}    Execute Command    pgrep c8y_configurati
    Set Suite Variable    ${pid}

Start MQTT Subscriber health-check & send empty message
   
    Write    sudo tedge mqtt sub 'tedge/health/c8y-configuration-plugin' &
    Write    sudo tedge mqtt pub 'tedge/health-check/c8y-configuration-plugin' ''
    ${output}=         Read    delay=1s
    Should Contain    ${output}    "pid":${pid}
    Should Contain    ${output}    "status":"up"

*** Keywords ***
Open Connection And Log In
   SSHLibrary.Open Connection     ${HOST}
   SSHLibrary.Login               ${USERNAME}        ${PASSWORD}
 