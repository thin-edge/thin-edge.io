#!/bin/bash

# change the owenership of the below directories/files to `tedge` user,
# as there is only `tedge` user exists.

if [ -d "/etc/tedge/operations/c8y" ]; then
    sudo chown tedge:tedge /etc/tedge/operations/c8y
    sudo chown tedge:tedge /etc/tedge/operations/c8y/c8y_*
fi

if [ -d "/etc/tedge/operations/az" ]; then
    sudo chown tedge:tedge /etc/tedge/operations/az
fi

if [ -d "/etc/tedge/.agent/" ]; then
    sudo chown tedge:tedge /etc/tedge/.agent
fi

if [ -d "/var/log/tedge/agent/" ]; then
    sudo chown tedge:tedge /var/log/tedge/agent
fi

if [ -f "/run/lock/tedge_agent.lock" ]; then
    sudo chown tedge:tedge /run/lock/tedge_agent.lock
fi

if [ -f "/run/lock/tedge-mapper-c8y.lock" ]; then
    sudo chown tedge:tedge /run/lock/tedge-mapper-c8y.lock
fi

if [ -f "/run/lock/tedge-mapper-az.lock" ]; then
    sudo chown tedge:tedge /run/lock/tedge-mapper-az.lock
fi

if [ -f "/run/lock/tedge-mapper-aws.lock" ]; then
    sudo chown tedge:tedge /run/lock/tedge-mapper-aws.lock
fi

if [ -f "/run/lock/tedge-mapper-collectd.lock" ]; then
    sudo chown tedge:tedge /run/lock/tedge-mapper-collectd.lock
fi
