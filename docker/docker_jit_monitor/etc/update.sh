#/bin/sh
if ! git pull | grep -q "Already up to date"; then
    echo "Updating docker image testing2."
    ./build.sh --sdk-path ~/sdk.zip
fi
