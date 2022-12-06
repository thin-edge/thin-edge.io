#Command to execute:    robot -d \results --timestampoutputs --log http_file_transfer_api.html --report NONE -v BUILD:840 -v HOST:192.168.1.130 thin-edge.io/tests/RobotFramework/tedge/http_file_transfer_api.robot
*** Settings ***
Library    Browser
Library    OperatingSystem
Library    Dialogs
Library    SSHLibrary
Library    DateTime
Library    CryptoLibrary    variable_decryption=True
Suite Setup            Open Connection And Log In
Suite Teardown         Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=    
${DeviceID}       
${Version}        0.*
${download_dir}    /home/pi/
${url_dow}    https://github.com/thin-edge/thin-edge.io/actions
${user_git}    crypt:3Uk76kNdyyYOXxus2GFoLf8eRlt/W77eEkcSiswwh04HNfwt0NlJwI7ATKPABmxKk8K1a8NsI5QH0w8EmT8GWeqrFwX2    
${pass_git}    crypt:IcTs6FyNl16ThjeG6lql0zNTsjCAwg5s6PhjRrcEwQ9DVHHRB4TjrGcpblR6R1v7j9oUlL3RzwxGpfBfsijVnQ==    
${FILENAME}
${DIRECTORY}    /home/pi/
${url}    https://thin-edge-io.eu-latest.cumulocity.com/
${url_tedge}    thin-edge-io.eu-latest.cumulocity.com
${user}    systest_preparation
${pass}    crypt:34mpoxueRYy/gDerrLeBThQ2wp9F+2cw50XaNyjiGUpK488+1fgEfE6drOEcR+qZQ6dcjIWETukbqLU=    
${BUILD}
${ARCH}
${dir}

*** Test Cases ***
Create Timestamp    #Creating timestamp to be used for Device ID
        ${timestamp}=    get current date    result_format=%d%m%Y%H%M%S
        log    ${timestamp}
        Set Global Variable    ${timestamp}
Define Device id    #Defining the Device ID, structure is (ST'timestamp') (eg. ST01092022091654)
        ${DeviceID}   Set Variable    ST${timestamp}
        Set Suite Variable    ${DeviceID}
Check Architecture    #Checking the architecture in order to download the right SW
    ${output}=    Execute Command   uname -m
    ${ARCH}    Set Variable    ${output}
    Set Global Variable    ${ARCH}

Set File Name    #Setting the file name for download
    Run Keyword If    '${ARCH}'=='aarch64'    aarch64
    ...  ELSE    armv7   
Uninstall tedge with purge
    Execute Command    wget https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh
    Execute Command    chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge

Clear previous downloaded files if any
    Execute Command    rm *.deb | rm *.zip | rm *.sh*

Download the Build Package
    New Context    acceptDownloads=True
    New Page    ${url_dow} 
    Click    //a[normalize-space()='Sign in']
    Fill Text    //input[@id='login_field']    ${user_git}
    Fill Text    //input[@id='password']    ${pass_git}
    Click    //input[@name='commit']
    # Pause Execution
    Fill Text    //input[@placeholder='Filter workflow runs']    workflow:build-workflow is:success 
    Keyboard Key    press    Enter   
    Sleep    5s
    Wait For Elements State    //*[contains(@aria-label, '${BUILD}')]    visible
    Click    //*[contains(@aria-label, '${BUILD}')]
    Sleep    5s
    Wait For Elements State     //a[normalize-space()='${FILENAME}']    visible
    ${dl_promise}          Promise To Wait For Download    ${DIRECTORY}${FILENAME}.zip
    Click    //a[normalize-space()='${FILENAME}']
    ${file_obj}=    Wait For  ${dl_promise}
    Sleep    5s

Copy File to Device
    Put File    ${download_dir}${FILENAME}.zip

Unpack the File
    ${rc}=    Execute Command    unzip ${FILENAME}.zip    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Mosquitto
    ${rc}=    Execute Command    sudo apt-get --assume-yes install mosquitto    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Libmosquitto1
    ${rc}=    Execute Command    sudo apt-get --assume-yes install libmosquitto1    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Collectd-core
    ${rc}=    Execute Command    sudo apt-get --assume-yes install collectd-core    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

thin-edge.io installation
    ${rc}=    Execute Command    sudo dpkg -i ./tedge_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Tedge mapper
    ${rc}=    Execute Command    sudo dpkg -i ./tedge-mapper_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Tedge agent
    ${rc}=    Execute Command    sudo dpkg -i ./tedge-agent_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Tedge apama plugin
    ${rc}=    Execute Command    sudo dpkg -i ./tedge-apama-plugin_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install tedge apt plugin
   ${rc}=    Execute Command    sudo dpkg -i ./tedge-apt-plugin_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Tedge logfile request plugin
   ${rc}=    Execute Command    sudo dpkg -i ./c8y-log-plugin_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install C8y configuration plugin
    ${rc}=    Execute Command    sudo dpkg -i ./c8y-configuration-plugin_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Install Watchdog
    ${rc}=    Execute Command    sudo dpkg -i ./tedge-watchdog_${Version}_arm*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Set external MQTT bind address
    Execute Command    sudo tedge config set mqtt.external.bind_address ${HOST}
    

Check MGTT bind address
    ${bind}    Execute Command    tedge config get mqtt.external.bind_address
    Should Be Equal    ${bind}    ${HOST}

Create directory
    Execute Command    sudo mkdir /var/tedge/file-transfer

Get Put Delete   
    Child Connection
    Execute Command    curl -X PUT -d "test of put" http://${HOST}:80/tedge/file-transfer/file_a
    ${get}=    Execute Command    curl http://${HOST}:80/tedge/file-transfer/file_a
    Should Be Equal    ${get}    test of put
    Execute Command    curl -X DELETE http://${HOST}:80/tedge/file-transfer/file_a


Delete created directory
    Execute Command    sudo rm -rf /var/tedge/file-transfer


*** Keywords ***
Open Connection And Log In
   Open Connection     ${HOST}
   Login               ${USERNAME}        ${PASSWORD}

Child Connection
   Open Connection     192.168.1.110
   Login               ${USERNAME}        ${PASSWORD}
aarch64
    [Documentation]    Setting file name according architecture
    ${FILENAME}    Set Variable    debian-packages-aarch64-unknown-linux-gnu
    Log    ${FILENAME}
    Set Global Variable    ${FILENAME}
armv7
    [Documentation]    Setting file name according architecture
    ${FILENAME}    Set Variable    debian-packages-armv7-unknown-linux-gnueabihf
    Log    ${FILENAME}
    Set Global Variable    ${FILENAME}
