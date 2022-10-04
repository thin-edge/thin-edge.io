#Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/config_management/inotify_crate.robot

*** Settings ***
Library    Browser
Library    SSHLibrary 
Library    CryptoLibrary    variable_decryption=True
Library    Dialogs
Suite Setup            Open Connection And Log In
Suite Teardown         Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=
${url}    https://thin-edge-io.eu-latest.cumulocity.com/
${user}    systest_preparation
${pass}    crypt:34mpoxueRYy/gDerrLeBThQ2wp9F+2cw50XaNyjiGUpK488+1fgEfE6drOEcR+qZQ6dcjIWETukbqLU= 
${toml}    "files = [\n    { path = '/etc/tedge/tedge.toml', type = 'tedge.toml'},\n    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },\n    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },\n    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },\n    { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }\n]"

*** Test Cases ***
starting the Configuration plugin process
    Execute Command   sudo systemctl start c8y-configuration-plugin.service 

Navigate to Cumulocity Device Management
    New Page    ${url}
    Wait For Elements State    //button[normalize-space()='Log in']    visible
    Type Text    id=user    ${user}
    Type Text    id=password    ${pass}
    Click    //button[normalize-space()='Log in']
    Wait For Elements State    //i[@class='icon-2x dlt-c8y-icon-th']    visible
    Click    //i[@class='icon-2x dlt-c8y-icon-th']
    Wait For Elements State    //span[normalize-space()='Device management']    visible
    Click    //span[normalize-space()='Device management']
    Wait For Elements State    //span[normalize-space()='Devices']    visible
# Navigate to the desired Device
    Click    //span[normalize-space()='Devices']
    Wait For Elements State    //span[normalize-space()='All devices']    visible
    Click    //span[normalize-space()='All devices']
    Wait For Elements State    //td[@ng-class='table-cell-truncate']    visible
    Click    //td[@ng-class='table-cell-truncate']
    Wait For Elements State    //span[normalize-space()='Configuration']    visible
# open Devices Configuration tab
    Click    //span[normalize-space()='Configuration']
    Wait For Elements State    //span[@title='c8y-configuration-plugin']    visible    
# c8y-configuration-plugin is listed as supported configuration type
    Click    //span[@title='c8y-configuration-plugin']
    Wait For Elements State    //button[@id='action-btn']    visible
    Click    //button[@id='action-btn']
# change the configuration file
    Execute Command    sudo rm /etc/tedge/c8y/c8y-configuration-plugin.toml
    Execute Command    sudo printf ${toml} > c8y-configuration-plugin.toml
    Execute Command    sudo mv c8y-configuration-plugin.toml /etc/tedge/c8y/
    # Execute Command    sudo systemctl restart c8y-configuration-plugin.service    #########REMOVE AFTER BUGFIX OF #1414###############
    Reload
    Wait For Elements State    //body/c8y-ui-root[@id='app']/c8y-bootstrap/div/div/div/c8y-context-route/c8y-device-configuration/div/tabset/div/tab[@role='tabpanel']/div/div/c8y-device-configuration-list/div[1]    visible
    ${text}=    Get Text    //body/c8y-ui-root[@id='app']/c8y-bootstrap/div/div/div/c8y-context-route/c8y-device-configuration/div/tabset/div/tab[@role='tabpanel']/div/div/c8y-device-configuration-list/div[1]
    Should Contain    ${text}    c8y-bridge.conf
    Should Contain    ${text}    c8y-configuration-plugin
    Should Contain    ${text}    mosquitto.conf
    Should Contain    ${text}    tedge-mosquitto.conf
    Should Contain    ${text}    tedge.toml
    Should Contain    ${text}    example

*** Keywords ***
Open Connection And Log In
   Open Connection     ${HOST}
   Login               ${USERNAME}        ${PASSWORD}
