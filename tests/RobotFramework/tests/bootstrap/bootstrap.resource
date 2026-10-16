*** Settings ***
Documentation       Keywords shared by the bootstrap suites

Resource            ../../resources/common.resource
Library             ThinEdgeIO


*** Variables ***
${EXAMPLES}     ${CURDIR}/examples
${HOOKS}        /usr/share/tedge/bootstrap.d


*** Keywords ***
Bootstrap Log
    [Documentation]    The log file of the most recent bootstrap run,
    ...    holding the detail the console summarizes
    ...    (its path is printed in the run's summary)
    ${log}=    Execute Command    cmd=cat "$(ls -t /var/log/tedge/tedge-bootstrap-*.log | head -1)"
    RETURN    ${log}

Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN
    Execute Command
    ...    cmd=mkdir -p ${HOOKS}/prepare.d ${HOOKS}/configure.d ${HOOKS}/register.d ${HOOKS}/finalize.d ${HOOKS}/clouds.d

Install Example Files
    [Documentation]    Install one file of an example into a bootstrap.d
    ...    drop-in directory, keeping hooks executable
    [Arguments]    ${example_path}    ${target_dir}
    Transfer To Device    ${EXAMPLES}/${example_path}    ${HOOKS}/${target_dir}/
    IF    "${target_dir}" != "clouds.d"
        Execute Command    cmd=chmod +x ${HOOKS}/${target_dir}/*
    END
