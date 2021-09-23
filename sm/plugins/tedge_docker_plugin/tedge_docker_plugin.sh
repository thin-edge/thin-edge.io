#!/bin/sh

set -e

COMMAND="$1"
IMAGE_NAME="$2"
IMAGE_TAG="$3"

case "$COMMAND" in
    prepare)
        # nothing to do here
        ;;
    list)
        sudo docker image list --format '{"name":"{{.Repository}}","version":"{{.Tag}}"}'
        ;;
    install)
        # Stop all containers using the provided image name
        containers=sudo docker ps --format "{{.ID}} {{.Image}}" | grep $IMAGE_NAME | awk '{print $1}'
        for container in $containers
        do
            sudo docker stop $container
        done
        # Spawn new containers with the provided image name and version to replace the stopped ones
        sudo docker run $IMAGE_NAME:$IMAGE_TAG
        ;;
    remove)
        containers=sudo docker ps --format "{{.ID}} {{.Image}}" | grep $IMAGE_NAME:$IMAGE_TAG | awk '{print $1}'
        if [ -z $containers ]
        then
            echo "No containers found for the image: $IMAGE_NAME:$IMAGE_TAG"
        fi
        for container in $containers
        do
            sudo docker stop $container
        done
        ;;
    finalize)
        # Prune all the unused containers. The --force command is used to avoid a [y/N] user prompt
        sudo docker container prune --force
        # Prune all the unused images
        sudo docker image prune --all --force
        ;;
    *)
        echo "Unsupported argument: $COMMAND"
        exit 1
        ;;
esac
exit 0
