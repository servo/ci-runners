# Setting up a server on Hetzner

Overview of the server scripts:

- `server/build-nixos-installer-kexec.sh`
  <br>From any existing NixOS system, build a NixOS installer kexec image.
- `server/start-nixos-installer.sh`
  <br>From the Hetzner rescue system, build and run the NixOS installer.
- `server/first-time-install.sh <hostname> <disk> [disk ...]`
  <br>From the NixOS installer image, wipe the given disks and install NixOS.
- `server/install-or-reinstall.sh <hostname> <path/to/mnt>`
  <br>From the NixOS installer image, install or reinstall NixOS to the given root filesystem mount, without wiping any disks. Won’t run correctly on the deployed server.

Start the [rescue system](https://docs.hetzner.com/robot/dedicated-server/troubleshooting/hetzner-rescue-system/), then connect over SSH (use `ssh -oUserKnownHostsFile=/dev/null`) and run the following:

```
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ apt update
$ apt install -y zsh
$ ./start-nixos-installer.sh
```

When you see `+ kexec -e`, kill your SSH session by pressing **Enter**, `~`, `.`, then reconnect over SSH (use `ssh -4 -oUserKnownHostsFile=/dev/null` this time) and run the following:

```
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ ./first-time-install.sh ci0 /dev/nvme{0,1}n1
$ reboot
```

Now you can [set up the monitor service](#setting-up-the-monitor-service). Note that rebooting may not be enough to terminate the Hetzner rescue system. If the rescue system is still active, try **Reset** > **Execute an automatic hardware reset** in the Hetzner console.
