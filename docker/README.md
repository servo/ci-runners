# Docker images for servo

This folder contains a number of docker images used for a CI-pipeline to build and test servo on
OpenHarmony and HarmonyOS devices.
The images can be built by running `build.sh` in this folder. 
The images are not intended for publishing.

* base: A slim base image, that the other images inherit from
* gh_runner: Contains the GitHub actions runner (needed by multiple images)
* hos_commandline_tools: A helper docker image containing the commandline tools for HarmonyOS,
  which are needed by multiple images.
* hos_builder: An image which can build servo for HarmonyOS
* runner: An image which can sign, flash and run servo on an OpenHarmony / HarmonyOS device.
          This image contains some (minor) secrets like the developer signing key.