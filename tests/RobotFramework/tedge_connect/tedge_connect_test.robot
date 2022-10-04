*** Settings ***
Documentation    Run connection test while being connected and check the positive response in stdout
...              disconnect the device from cloud and check the negative message in stderr
...              Run sudo tedge connect c8y and check 

Library    SSHLibrary
Library    CryptoLibrary    variable_decryption=True
Library    Dialogs

Suite Setup            Open Connection And Log In
Suite Teardown         SSHLibrary.Close All Connections


*** Variables ***
${HOST}           
${USERNAME}       pi    
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=

*** Tasks ***
tedge_connect_test_positive
    Execute Command    sudo tedge connect c8y    #Connecting to Cumulocity IoT
    ${stdout}=    Execute Command    sudo tedge connect c8y --test    #Testing the status of the connection
    Should Contain    ${stdout}    Connection check to c8y cloud is successful.    #Expected message
    Log    ${stdout}

tedge_connect_test_negative
    Execute Command    sudo tedge disconnect c8y    #Disonnecting from Cumulocity IoT
    ${stdout}    ${stderr}=    Execute Command    sudo tedge connect c8y --test    return_stderr=True    #Testing the status of the connection
    Should Contain    ${stderr}    Error: failed to test connection to Cumulocity cloud.    #Expected message
    Log    ${stderr}

tedge_connect_test_sm_services
    ${stdout}    Execute Command    sudo tedge connect c8y    #Connecting to Cumulocity IoT
    Should Contain    ${stdout}    Successfully created bridge connection!    #Expected message
    Should Contain    ${stdout}    tedge-agent service successfully started and enabled!    #Expected message
    Should Contain    ${stdout}    tedge-mapper-c8y service successfully started and enabled!    #Expected message
    Log    ${stdout}
tedge_disconnect_test_sm_services
    ${stdout}    Execute Command    sudo tedge disconnect c8y    #Disonnecting from Cumulocity IoT
    Should Contain    ${stdout}    Cumulocity Bridge successfully disconnected!    #Expected message
    Should Contain    ${stdout}    tedge-agent service successfully stopped and disabled!    #Expected message
    Should Contain    ${stdout}    tedge-mapper-c8y service successfully stopped and disabled!    #Expected message
    Log    ${stdout}

*** Keywords ***
Open Connection And Log In
   
    SSHLibrary.Open Connection     ${HOST}
    SSHLibrary.Login               ${USERNAME}        ${PASSWORD}
