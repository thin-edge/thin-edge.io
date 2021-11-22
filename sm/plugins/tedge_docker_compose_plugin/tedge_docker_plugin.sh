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

extract_docker_compose_path_from_args() {
    COMPOSE_ARG="$1"
    if [ -z "$COMPOSE_ARG" ]; then
        echo "docker-compose.yaml path is a mandatory argument"
        exit 1
    fi
    shift   # Pop image name from args list
    COMPOSE_FILE=$COMPOSE_ARG
    
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
        docker image list --format '{{.Repository}}\t{{.Tag}}' || exit 2
        ;;
    install)
        # Extract the docker image tag into the IMAGE_TAG variable
        echo $@

        #COMPOSE_FILE=$1
        extract_docker_compose_path_from_args $@

        # Spawn new containers with the provided image name and version to replace the stopped one
        echo "Compose file" $COMPOSE_FILE
        sudo docker-compose -f $COMPOSE_FILE up -d || exit 2
        ;;
    remove)
        #COMPOSE_FILE=$1
        extract_docker_compose_path_from_args $@
        sudo docker-compose -f $COMPOSE_FILE down || exit 2
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
