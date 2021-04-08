#!/bin/sh
set -e

#VERSION="0.1.0"
#ARCH="armhf"
VERSION=$1
ARCH=$(dpkg --print-architecture)

BLUE='\033[1;34m'
COLORRESET='\033[0m'

echo "${BLUE}Thank you for trying thin-edge.io! ${COLORRESET}\n"

if [ -z "$VERSION" ]
then
    echo "Please use this script with the version as argument."
    echo "For example: ${BLUE}sudo ./get-thin-edge_io.sh 0.1.0${COLORRESET}"
    exit 0
fi

if [ "$ARCH" = "armhf" ] || [ "$ARCH" = "amd64"]
then
    echo "${BLUE}Installing for architecture $ARCH ${COLORRESET}"
else
    echo "$ARCH is currently not supported. Currently supported are armhf and amd64."
    exit 0
fi

if [ -d "/tmp/tedge" ]
then
    rm -R /tmp/tedge
fi

echo "${BLUE}Installing mosquitto as prerequirement for thin-edge.io${COLORRESET}"
apt install mosquitto -y

wget https://github.com/thin-edge/thin-edge.io/releases/download/0.1.0/tedge_${VERSION}_${ARCH}.deb -P /tmp/tedge
wget https://github.com/thin-edge/thin-edge.io/releases/download/0.1.0/tedge_mapper_${VERSION}_${ARCH}.deb -P /tmp/tedge

dpkg -i /tmp/tedge/tedge_${VERSION}_${ARCH}.deb
dpkg -i /tmp/tedge/tedge_mapper_${VERSION}_${ARCH}.deb

rm -R /tmp/tedge
echo ""
echo "To administrate your thin-edge.io installation your user has to be part of the group 'tedge-users'."
echo "You can add your user to this group with the command${BLUE} 'adduser <your-user> tedge-users'${COLORRESET}.\n"

echo "You can go to our documentation to find next steps:${BLUE} https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/howto-guides/003_registration.md ${COLORRESET}"
