#!/bin/sh
# Replace the tedge-agent and tedge-mapper-* services with a single `tedge` service running `tedge run all`
set -e

cat > /etc/systemd/system/tedge.service <<EOT
[Unit]
Description=thin-edge.io components running in one process (tedge run all)
After=syslog.target network.target mosquitto.service

[Service]
User=tedge
ExecStartPre=+-/usr/bin/tedge init
ExecStart=/usr/bin/tedge run all
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOT

# Masking (rather than just disabling) stops `tedge connect` from starting the per-component services again
systemctl stop tedge-agent 'tedge-mapper-*'
systemctl mask tedge-agent \
    tedge-mapper-c8y tedge-mapper-aws tedge-mapper-az tedge-mapper-local \
    tedge-mapper-c8y@ tedge-mapper-aws@ tedge-mapper-az@
systemctl daemon-reload
systemctl enable --now tedge
