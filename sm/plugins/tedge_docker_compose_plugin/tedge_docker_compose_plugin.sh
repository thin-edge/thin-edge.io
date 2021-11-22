#!/bin/sh

# Expected signature example: $sudo ./tedge_docker_compose_plugin.sh remove nextcloud-postgres
# File should be downloaded to /tmp/
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

DOCKER_COMPOSE_PLUGIN_PATH='/etc/tedge/sm-plugins/docker-compose/'
EXTENSION=".yaml"
TMP_PATH="/tmp/"

unsupported_args_check() {
    if ! [ -z $1 ]; then
        echo "Unsupported arguments: $@"
        exit 1
    fi
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
        ls $DOCKER_COMPOSE_PLUGIN_PATH | cut -d. -f1 || exit 2
        ;;
    install)
        # Extract the docker docker-compose path into the COMPOSE_FILE variable
        extract_docker_compose_path_from_args $@
        TMP_PATH="$TMP_PATH$COMPOSE_ARG"
        echo $TMP_PATH
        sudo cp $TMP_PATH $DOCKER_COMPOSE_PLUGIN_PATH
        COMPOSE_NAME="$(echo $TMP_PATH | cut -d/ -f3 | cut -d. -f1)"
        INSTALL_PATH="$DOCKER_COMPOSE_PLUGIN_PATH$COMPOSE_NAME"
        echo $INSTALL_PATH

        # Spawn new containers with the provided image name and version to replace the stopped one
        echo "Install path" $INSTALL_PATH
        sudo docker-compose -f $INSTALL_PATH up -d || exit 2
        ;;
    remove)
        # Extract the docker docker-compose path into the COMPOSE_FILE variable
        extract_docker_compose_path_from_args $@
        REMOVE_PATH="$DOCKER_COMPOSE_PLUGIN_PATH$COMPOSE_FILE"
        echo $REMOVE_PATH
    
        sudo docker-compose -f $REMOVE_PATH down || exit 2
        
        sudo rm $REMOVE_PATH
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
