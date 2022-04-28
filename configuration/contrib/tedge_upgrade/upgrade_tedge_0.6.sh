#!/bin/bash

# change the owenership of the below directories/files to `tedge` user,
# as there is only `tedge` user exists.

sudo chown tedge:tedge /etc/tedge/operations/c8y/c8y_*
sudo chown tedge:tedge /etc/tedge/operations/az
sudo chown tedge:tedge /etc/tedge/.agent
sudo chown tedge:tedge /var/log/tedge/agent
sudo chown tedge:tedge /run/lock/tedge_agent.lock
sudo chown tedge:tedge /run/lock/tedge-mapper-c8y.lock
sudo chown tedge:tedge /run/lock/tedge-mapper-az.lock
sudo chown tedge:tedge /run/lock/tedge-mapper-collectd.lock
