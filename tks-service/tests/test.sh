#!/bin/bash
#
# This script is used to test the functionality of the tks-service
# service.
#
# It would first check if qdbus is installed. If not, it would issue an error.
# Then, it would check if another instance of org.freedesktop.secrets is
# availble. If yes, it would issue a warning, as that one might be offered by
# some other service provider.
#
# Then tks-service is being started. If it fails, an error is issued.
#

BIN_PATH=../../target/debug

# Check if qdbus is installed
if ! command -v qdbus &> /dev/null
then
    echo "qdbus could not be found"
    exit
fi

# Check if another instance of org.freedesktop.secrets is available
# If yes, issue a warning
# If no, start tks-service
if qdbus org.freedesktop.secrets &> /dev/null
then
    echo "WARNING: Another instance of org.freedesktop.secrets is available"
    # display the name of the process offerring the service
    qdbus org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus.GetConnectionUnixProcessID org.freedesktop.secrets
else
    echo "Starting tks-service"
    $(BIN_PATH)/tks-service &
fi

