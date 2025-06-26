*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Teardown       Get Logs    name=${DEVICE_SN}

Test Tags           theme:troubleshooting    theme:installation


*** Test Cases ***
Migrate Legacy Configuration Files
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN

    # Copy old c8y-log-plugin's config file before bootstrapping
    ThinEdgeIO.Execute Command    rm -f /etc/tedge/plugins/*
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/c8y/c8y-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    # touch file again to change last modified timestamp, otherwise the logfile retrieval could be outside of the requested range
    Execute Command
    ...    chown root:root /etc/tedge/c8y/c8y-log-plugin.toml /var/log/example/example.log && touch /var/log/example/example.log

    # Bootstrap tedge-agent so that it picks up the legacy c8y-log-plugin.toml
    Run Bootstrap    ${DEVICE_SN}
    Cumulocity.Device Should Exist    ${DEVICE_SN}

    ${operation}=    Cumulocity.Get Log File    example
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Run Bootstrap
    [Arguments]    ${external_id}
    ${bootstrap_cmd}=    ThinEdgeIO.Get Bootstrap Command
    Execute Command    cmd=${bootstrap_cmd}
    Register Device With Cumulocity CA    external_id=${external_id}
    Execute Command    cmd=tedge connect c8y
