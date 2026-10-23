#/bin/sh
if git pull | grep -q "Already up to date"; then

else
    ./build.sh --sdk-path ~/sdk-link.zip
fi
