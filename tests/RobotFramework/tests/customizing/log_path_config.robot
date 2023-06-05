#Command to execute:    robot -d \results --timestampoutputs --log log_path_config.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/customizinglog_path_config.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Setup
Suite Teardown         Get Logs

*** Test Cases ***
Stop tedge-agent service
    Execute Command    sudo systemctl stop tedge-agent.service
    Execute Command    sudo rm -f /run/lock/tedge*agent.lock    # BUG?: Stopping the service does not delete the file, so if starting tedge_agent as a different user causes problems!

Customize the log path
    Execute Command    sudo tedge config set logs.path /test

Initialize tedge-agent
    Start Service    tedge-agent

Check created folders
    Directory Should Exist    /test/tedge/agent

Remove created custom folders
    Execute Command    sudo rm -rf /test
