*** Settings ***
Documentation       Test suite for tedge-p11-server functionality.

Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
# Config dir and transient unit of the tedge-p11-server instance serving the no-login token
${NO_LOGIN_DIR}     /tmp/nologin-token
${NO_LOGIN_UNIT}    tedge-p11-server-nologin


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

    Execute Command    systemctl stop tedge-p11-server tedge-p11-server.socket
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=Failed to connect to tedge-p11-server UNIX socket at '/run/tedge-p11-server/tedge-p11-server.sock'

    Execute Command    systemctl start tedge-p11-server.socket

    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:object=nonexistent_key
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=PKCS #11 service failed: Failed to find a key
    Execute Command    cmd=tedge config unset c8y.device.key_uri

Prints version on startup
    Restart Service    tedge-p11-server
    ${stdout}=    Execute Command    tedge-p11-server --version    strip=True
    Logs Should Contain    Starting ${stdout}

Use a token which does not require a login
    [Documentation]    A token that doesn't set the CKF_LOGIN_REQUIRED flag gives access to its
    ...    objects without a login, and rejects C_Login (e.g. with CKR_USER_TYPE_INVALID), so no
    ...    login must be attempted for it and the configured PIN must be ignored.
    ...    The p11-kit trust module provides such a token.
    ...    A dedicated tedge-p11-server instance and config dir are used so the SoftHSM2 setup of
    ...    the other tests is left alone.
    ${module}=    Execute Command    ls /usr/lib/*/pkcs11/p11-kit-trust.so    strip=${True}
    Execute Command    mkdir -p ${NO_LOGIN_DIR}

    # tedge is statically linked and can't load a PKCS#11 module itself, so the module is loaded by
    # its own tedge-p11-server instance, which tedge reaches over that server's socket
    Execute Command
    ...    cmd=systemd-run --unit=${NO_LOGIN_UNIT} --collect /usr/bin/tedge-p11-server --config-dir ${NO_LOGIN_DIR} --module-path ${module} --pin not-a-valid-pin --socket-path ${NO_LOGIN_DIR}/p11.sock
    Wait Until Keyword Succeeds    10x    1s    Execute Command    test -S ${NO_LOGIN_DIR}/p11.sock

    Execute Command    tedge --config-dir ${NO_LOGIN_DIR} config set device.cryptoki.mode socket
    Execute Command
    ...    tedge --config-dir ${NO_LOGIN_DIR} config set device.cryptoki.socket_path ${NO_LOGIN_DIR}/p11.sock

    # The token holds no keys, but opening a session on it must succeed without logging in
    ${stderr}=    Execute Command
    ...    tedge --config-dir ${NO_LOGIN_DIR} hsm list-keys
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${stderr}    No keys were found on the token
    [Teardown]    Remove No Login Token Server


*** Keywords ***
Remove No Login Token Server
    [Documentation]    Stops the tedge-p11-server instance serving the no-login token.
    Execute Command    systemctl stop ${NO_LOGIN_UNIT}    ignore_exit_code=${True}
    Execute Command    rm -rf ${NO_LOGIN_DIR}

Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    Execute Command    cmd=/usr/bin/tedge-init-hsm.sh --type softhsm2 --pin 123456
    # tests expect that the device.key_uri is initially unset
    Execute Command    cmd=tedge config unset device.key_uri

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}

    Unset tedge-p11-server Uri
