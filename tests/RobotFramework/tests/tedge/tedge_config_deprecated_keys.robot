*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup         Custom Setup
Test Teardown      Get Logs

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
Migrate c8y.entity_store.* keys to agent.entity_store.* keys 1.4->1.5->2.0.0->latest
    [Documentation]    \#4191
    ...    In normal deprecation scenario, we don't need this kind of test.
    ...    However, since c8y.entity_store.* keys deprecation was partially done in 1.5.0,
    ...    this can cause the mix state that both c8y.entity_store.* and agent.entity_store.* keys exist at the same time,
    ...    and the values of these two keys can be different.
    ...    Hence, we need to verify the migration of these keys in this test case.
    # c8y.entity_store.* keys are used in 1.4.0
    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s -- 1.4.0
    Execute Command    sudo tedge config add c8y.entity_store.auto_register false
    Execute Command    sudo tedge config add c8y.entity_store.clean_start false

    # Update tedge to 1.5.0 which deprecates c8y.entity_store.* keys
    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s -- 1.5.0
    Execute Command    sudo tedge config add agent.entity_store.auto_register true
    Execute Command    sudo tedge config add agent.entity_store.clean_start true

    # Update tedge to 2.0.0 which migrates cloud specific keys to mapper config
    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s -- 2.0.0

    ${output_tedge_toml}=    Execute Command    cat /etc/tedge/tedge.toml
    Should Contain    ${output_tedge_toml}    auto_register = true
    Should Contain    ${output_tedge_toml}    clean_start = true

    # This entity_store.auto_register & clean_start are zombie configuration, cannot be modified by CLI
    ${output_c8y_mapper_toml}=    Execute Command    cat /etc/tedge/mappers/c8y/mapper.toml
    Should Contain    ${output_c8y_mapper_toml}    auto_register = false
    Should Contain    ${output_c8y_mapper_toml}    clean_start = false

    # The values from agent.entity_store.* keys should be effective, not the values from c8y.entity_store.* keys
    ${auto_register}=    Execute Command    tedge config get agent.entity_store.auto_register    strip=True
    Should Be Equal    ${auto_register}    true
    ${clean_start}=    Execute Command    tedge config get agent.entity_store.clean_start   strip=True
    Should Be Equal    ${clean_start}    true

    # Update to the test version
    Execute Command    apt-get install -y /setup/packages/tedge_*.deb
    Set Cumulocity URLs
    Register Device With Cumulocity CA    ${DEVICE_SN}
    Execute Command    tedge connect c8y

    # If tedge config migration error happens, these services will fail to start
    Service Health Status Should Be Up    tedge-agent
    Service Health Status Should Be Up    tedge-mapper-c8y

    # The zombie config is gone
    ${output_c8y_mapper_toml}=    Execute Command    cat /etc/tedge/mappers/c8y/mapper.toml
    Should Not Contain    ${output_c8y_mapper_toml}    auto_register = false
    Should Not Contain    ${output_c8y_mapper_toml}    clean_start = false

Migrate c8y.entity_store.* keys to agent.entity_store.* keys 1.4->1.5->latest
    [Documentation]    \#4191
    ...    The half deprecation is not solved in 2.0.0, but 2.0.0 migrates the cloud specific keys to mapper config.
    # c8y.entity_store.* keys are used in 1.4.0
    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s -- 1.4.0
    Execute Command    sudo tedge config add c8y.entity_store.auto_register false
    Execute Command    sudo tedge config add c8y.entity_store.clean_start false

    # Update tedge to 1.5.0 which deprecates c8y.entity_store.* keys
    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s -- 1.5.0
    Execute Command    sudo tedge config add agent.entity_store.auto_register true
    Execute Command    sudo tedge config add agent.entity_store.clean_start true

    # Update to the test version without stepping stone 2.0.0
    Execute Command    apt-get install -y /setup/packages/tedge_*.deb
    Set Cumulocity URLs
    Register Device With Cumulocity CA    ${DEVICE_SN}
    Execute Command    tedge connect c8y

    # If tedge config migration error happens, these services will fail to start
    Service Health Status Should Be Up    tedge-agent
    Service Health Status Should Be Up    tedge-mapper-c8y

    # There is no zombie config
    ${output_c8y_mapper_toml}=    Execute Command    cat /etc/tedge/mappers/c8y/mapper.toml
    Should Not Contain    ${output_c8y_mapper_toml}    auto_register = false
    Should Not Contain    ${output_c8y_mapper_toml}    clean_start = false


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    ${DEVICE_SN}
