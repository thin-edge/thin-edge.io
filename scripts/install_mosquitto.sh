#!/bin/bash

# Install Mosquitto
if ! sudo apt install -y mosquitto;
then
   exit 1
fi
