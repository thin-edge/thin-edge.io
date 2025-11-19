*** Settings ***
Documentation       Test suite for tedge-p11-server functionality.

Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
Ignore tedge.toml if missing
    Execute Command    rm -f ./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour as the user does not have to create a tedge.toml file)
    Should Not Contain    ${stderr}    Failed to read ./tedge.toml: No such file
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Ignore tedge.toml if empty
    Execute Command    touch ./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour, where the file is used for tedge and not tedge-p11-server)
    Should Not Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Ignore tedge.toml if incomplete
    Execute Command    echo '[device]' >./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour, where the file is used for tedge and not tedge-p11-server)
    Should Not Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    Should Not Contain    ${stderr}    missing field `cryptoki`
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Do not warn the user if tedge.toml is incomplete but not used
    Execute Command    rm -f ./tedge.toml
    ${stderr}=    Execute Command
    ...    tedge-p11-server --config-dir . --module-path xx.so --pin 11.pin --socket-path yy.sock --uri zz.uri
    ...    exp_exit_code=!0
    # Don't warn as all values are provided on the command line
    Should Not Contain    ${stderr}    Failed to read ./tedge.toml: No such file
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using the values provided on the command lin
    Should Contain    ${stderr}    xx.so
    Should Contain    ${stderr}    yy.sock
    Should Contain    ${stderr}    zz.uri

Warn the user if tedge.toml exists but cannot be read
    Execute Command    echo '[device.cryptoki]' >./tedge.toml
    Execute Command    chmod a-rw ./tedge.toml
    ${stderr}=    Execute Command
    ...    sudo -u tedge tedge-p11-server --config-dir . --module-path xx.so
    ...    exp_exit_code=!0
    # Warn the user
    Should Contain    ${stderr}    Failed to read ./tedge.toml: Permission denied
    # But proceed
    Should Contain    ${stderr}    Using cryptoki configuration

Warn the user if tedge.toml cannot be parsed
    Execute Command    rm -f ./tedge.toml
    Execute Command    echo '[corrupted toml ...' >./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Warn the user
    Should Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    # But proceed
    Should Contain    ${stderr}    Using cryptoki configuration


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/

    # initialize the soft hsm and create a certificate signing request
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    ${csr_path}=    Execute Command    cmd=tedge config get device.csr_path    strip=${True}
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}    csr_path=${csr_path}

    Unset tedge-p11-server Uri
