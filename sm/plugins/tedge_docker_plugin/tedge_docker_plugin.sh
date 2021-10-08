#!/bin/sh

usage() {
    cat << EOF
USAGE:
    docker <SUBCOMMAND>

SUBCOMMANDS:
    list           List all the installed modules
    prepare        Prepare a sequences of install/remove commands
    install        Install a module
    remove         Uninstall a module
    finalize       Finalize a sequences of install/remove commands
EOF
}

unsupported_args_check() {
    if ! [ -z $1 ]; then
        echo "Unsupported arguments: $@"
        exit 1
    fi
}

extract_image_tag_from_args() {
    IMAGE_NAME="$1"
    if [ -z "$IMAGE_NAME" ]; then
        echo "Image name is a mandatory argument"
        exit 1
    fi
    shift   # Pop image name from args list
    IMAGE_TAG=$IMAGE_NAME
    
    if ! [ -z $1 ]; then
        case "$1" in 
            --module-version)
                IMAGE_VERSION="$2"
                IMAGE_TAG=$IMAGE_NAME:$IMAGE_VERSION
                shift 2  # Pop --version and the version value from the args list
                ;;
            *)
                echo "Unsupported argument option: $1"
                exit 1
                ;;
        esac
    fi
    
    unsupported_args_check $@

}

if [ -z $1 ]; then
    echo "Provide at least one subcommand\n"
    usage
    exit 1
fi

COMMAND="$1"
shift   # Pop the command from args list

case "$COMMAND" in
    prepare)
        unsupported_args_check $@
        # nothing to do here
        ;;
    list)
        unsupported_args_check $@
        docker image list --format '{"name":"{{.Repository}}","version":"{{.Tag}}"}' || exit 2
        ;;
    install)
        # Extract the docker image tag into the IMAGE_TAG variable
        extract_image_tag_from_args $@

        # Stop all containers using the provided image name
        containers=$(docker ps -a --format "{{.ID}} {{.Image}}" | grep $IMAGE_NAME | awk '{print $1}') || exit 2
        if [ -z "$containers" ]
        then
            echo "No containers to update. Spawning a new one."
            docker run -d $IMAGE_TAG || exit 2
        else
            echo "Updating existing containers."
            for container in $containers
            do
                docker rm $(docker stop "$container") || exit 2

                # Spawn new containers with the provided image name and version to replace the stopped one
                docker run -d $IMAGE_TAG || exit 2
            done
        fi
        ;;
    remove)
        extract_image_tag_from_args $@

        containers=$(docker ps -a --format "{{.ID}} {{.Image}}" | grep $IMAGE_TAG | awk '{print $1}') || exit 2
        if [ -z "$containers" ]
        then
            echo "No containers found for the image: $IMAGE_TAG"
        fi
        for container in $containers
        do
            docker rm $(docker stop "$container") || exit 2
        done
        ;;
    finalize)
        unsupported_args_check $@
        # Prune all the unused images. The --force command is used to avoid a [y/N] user prompt
        docker image prune --all --force || exit 2
        ;;
    *)
        echo "Unsupported command: $COMMAND"
        exit 1
        ;;
esac
