# Docker image based CI for servo

This folder contains a number of docker images used for a CI-pipeline to build and test servo on
OpenHarmony and HarmonyOS devices.
In addition there is also a `docker_jit_monitor` Cargo project, which is responsible for
managing the docker based github runners.

The images can be built by running `build.sh` in this folder.
The images are not intended for publishing.

* base: A slim base image, that the other images inherit from
* gh_runner: Contains the GitHub actions runner (needed by multiple images)
* hos_commandline_tools: A helper docker image containing the commandline tools for HarmonyOS,
  which are needed by multiple images.
* hos_builder: An image which can build servo for HarmonyOS
* runner: An image which can sign, flash and run servo on an OpenHarmony / HarmonyOS device.
          This image contains some (minor) secrets like the developer signing key.


## Prepare the runner image

Before building the runner image for the first time, some manual steps are necessary:

On the host machine install `hdc`, connect a device and run `hdc shell`.
Accept the permission prompt on the device.
Then copy ~/.harmony/hdckey and ~/.harmony/hdckey.pub to the runner directory.
This allows the JIT runners to connect to the hdc device without a permission prompt.

To setup the signing configuration, create an archive called `ohos-config.tar` from the
`.ohos/config` directory and copy it together with the `signing-configs.json` into the
`runner` directory.


## Docker JIT monitor

This monitor starts github runners to build and test on OpenHarmony devices.
The Host system should be configured in the following way:

- Use rootless docker to reduce the attack surface (see https://docs.docker.com/engine/security/rootless/).
- The user starting the monitor should have minimal permissions
- Follow https://book.servo.org/hacking/building-for-openharmony.html#configuring-hdc-on-linux
  and allow the user running the monitor to access hdc devices.

Use the servo_ci.service file and put it into `~/.config/systemd/user`.
Edit the `GITHUB_TOKEN` and `SERVO_CI_GITHUB_SCOPE` variables.
Run `systemctl --user daemon-reload`
Enable linger by `loginctl enable-linger <USER>` for your user.
Start the service with `systemctl --user start servo_ci`. You will find logs in `journalctl`.
You can enable it to start at boot by running `systemctl --user enable servo_ci`.

