#Command to execute:    robot -d \results --timestampoutputs --log log_path_config.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/customizinglog_path_config.robot

*** Settings ***
Library    SSHLibrary 
Library    MQTTLibrary
Library    CryptoLibrary    variable_decryption=True
Suite Setup            Open Connection And Log In
Suite Teardown         SSHLibrary.Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=


*** Tasks ***
Stop tedge-agent service
    ${rc}=    Execute Command    sudo systemctl stop tedge-agent.service    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Customize the log path
    ${rc}=    Execute Command    sudo tedge config set logs.path /test    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Initialize tedge_agent
    ${rc}=    Execute Command    sudo tedge_agent --init    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Check created folders
    ${rc}=    Execute Command    cd /test/tedge/agent    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Remove created custom folders
    ${rc}=    Execute Command    sudo rm -rf /test    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

*** Keywords ***
Open Connection And Log In
   Open Connection     ${HOST}
   Login               ${USERNAME}        ${PASSWORD}
