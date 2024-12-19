GitHub Actions runners for Servo
================================

This repo contains:

- Server config and install scripts
    - `server/nixos` is the NixOS config
- Templates for CI runner images
    - `windows10/*` is for **Windows 10** runners
    - `ubuntu2204/*` is for **Ubuntu 22.04** runners
- Scripts for building CI runner images
    - `*/configure-base.sh`
    - `*/configure-runner.sh`
- Scripts for creating and managing runners
    - `create-runner.sh` creates and registers a new runner
    - `destroy-runner.sh` destroys the libvirt guest and ZFS volume for a runner
    - `register-runner.sh` registers a runner with the GitHub API
    - `unregister-runner.sh` unregisters a runner with the GitHub API
    - `mount-runner.sh` mounts the main filesystem of a runner on the host
- A service that automates runner management
    - `monitor` is the service
    - `.env.example` contains the settings

Setting up a server on Hetzner
------------------------------

Overview of the server scripts:

- `server/build-nixos-installer-kexec.sh`
  <br>From any existing NixOS system, build a NixOS installer kexec image.
- `server/start-nixos-installer.sh`
  <br>From the Hetzner rescue system, build and run the NixOS installer.
- `server/first-time-install.sh <hostname> <disk> [disk ...]`
  <br>From the NixOS installer image, wipe the given disks and install NixOS.
- `server/install-or-reinstall.sh <hostname> <path/to/mnt>`
  <br>From the NixOS installer image, install or reinstall NixOS to the given root filesystem mount, without wiping any disks. Won’t run correctly on the deployed server.
- `server/deploy.sh`
  <br>On the deployed server, deploy any NixOS config changes.
- `server/update.sh`
  <br>On the deployed server, pull the config from GitHub and deploy it.

Start the [rescue system](https://docs.hetzner.com/robot/dedicated-server/troubleshooting/hetzner-rescue-system/), then run the following:

```
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ apt install -y zsh
$ ./start-nixos-installer.sh
```

Reconnect over SSH, then run the following:

```
$ nix-shell -p git zsh jq
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ ./first-time-install.sh ci0 /dev/nvme{0,1}n1
$ reboot
```

Reconnect over SSH again, then run the following:

```
$ git clone https://github.com/servo/ci-runners.git /config
$ /config/server/update.sh
```

Setting up the monitor service
------------------------------

To get a GITHUB_TOKEN for the monitor service:

- [Create](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
    - Token name: `servo/ci-runners ci0`
    - Expiration: **7 days**
    - Resource owner: **servo**
    - Repository access > All repositories
    - Repository permissions > **Administration** > Access: **Read and write**
    - Organization permissions > **Self-hosted runners** > Access: **Read and write**

To set up the monitor service, run the following:

```
$ zfs create tank/base
$ zfs create tank/ci
$ virsh net-define cinet.xml
$ virsh net-autostart cinet
$ virsh net-start cinet

$ rustup default stable
$ git clone https://github.com/servo/servo.git ~/servo
$ cp /config/.env.example /config/.env
$ cp /config/monitor/monitor.toml.example /config/monitor/monitor.toml
$ vim -p /config/.env /config/monitor/monitor.toml
$ cd /config
$ RUST_LOG=debug cargo run
```

Windows 10 runner
-----------------

Runners created from this image preinstall all dependencies (including those specified in the main repo, like GStreamer and Chocolatey deps), preload the main repo, and prebuild Servo in the release profile.

To build the base vm, first build a clean image:

- Download images into /var/lib/libvirt/images
    - Windows 10 (multi-edition ISO), English (United States): [Win10_22H2_English_x64v1.iso](https://www.microsoft.com/en-us/software-download/windows10ISO) (sha256 = a6f470ca6d331eb353b815c043e327a347f594f37ff525f17764738fe812852e)
    - VirtIO drivers: [virtio-win-0.1.240.iso](https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.240-1/virtio-win-0.1.240.iso) (sha256 = ebd48258668f7f78e026ed276c28a9d19d83e020ffa080ad69910dc86bbcbcc6)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-windows10.clean`
    - `virsh define windows10.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-windows10.init -n servo-windows10.clean -f /dev/zvol/tank/base/servo-windows10.clean`
    - `virsh undefine servo-windows10.init`
- Install Windows:
    - `genisoimage -J -o /var/lib/libvirt/images/servo-windows10.config.iso windows10/autounattend.xml`
    - `virsh start servo-windows10.clean`
    - Wait for the guest to shut down
- Take a snapshot: `zfs snapshot tank/base/servo-windows10.clean@oobe`

Then build the base image:

- Clone the clean image: `zfs clone tank/base/servo-windows10{.clean@oobe,.new}`
- Create a temporary libvirt guest: `virt-clone --preserve-data --check path_in_use=off -o servo-windows10.clean -n servo-windows10.new -f /dev/zvol/tank/base/servo-windows10.new`
- Update new base image: `./mount-runner.sh servo-windows10.new $PWD/windows10/configure-base.sh`
- Take a snapshot: `zfs snapshot tank/base/servo-windows10.new@configure-base`
- Boot temporary vm guest: `virsh start servo-windows10.new`
- Wait for the guest to shut down, which indicates Servo was built successfully
- Take another snapshot: `zfs snapshot tank/base/servo-windows10.new@ready`
- Destroy the old base image (if it exists): `zfs destroy -r tank/base/servo-windows10`
- Rename the new base image: `zfs rename tank/base/servo-windows10{.new,}`
- Undefine the temporary libvirt guest: `virsh undefine servo-windows10.new`
- Create the base libvirt guest (if it doesn’t exist): `virt-clone --preserve-data --check path_in_use=off -o servo-windows10.clean -n servo-windows10 -f /dev/zvol/tank/base/servo-windows10`

To clone and start a new runner:

```sh
$ ./create-runner.sh servo-windows10 ready windows10
```

Ubuntu runner
-------------

To build the base vm, first build a clean image:

- Download images into /var/lib/libvirt/images
    - Ubuntu Server 22.04 cloud image: [jammy-server-cloudimg-amd64.img](https://cloud-images.ubuntu.com/jammy/20241217/jammy-server-cloudimg-amd64.img) (sha256 = 0d8345a343c2547e55ac815342e6cb4a593aa5556872651eb47e6856a2bb0cdd)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-ubuntu2204.clean`
    - `virsh define ubuntu2204.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.init -n servo-ubuntu2204.clean -f /dev/zvol/tank/base/servo-ubuntu2204.clean`
    - `virsh undefine servo-ubuntu2204.init`
- Install Ubuntu:
    - `qemu-img convert -f qcow2 -O raw jammy-server-cloudimg-amd64.{img,raw}`
    - `dd status=progress bs=1M if=jammy-server-cloudimg-amd64.raw of=/dev/zvol/tank/base/servo-ubuntu2204.clean`
    - `genisoimage -V CIDATA -R -o /var/lib/libvirt/images/servo-ubuntu2204.config.iso ubuntu2204/{user-data,meta-data}`
    - `virsh start servo-ubuntu2204.clean`
    - Wait for the guest to shut down
- Take a snapshot: `zfs snapshot tank/base/servo-ubuntu2204.clean@fresh-install`

Then build the base image:

- Clone the clean image: `zfs clone tank/base/servo-ubuntu2204{.clean@fresh-install,.new}`
- Create a temporary libvirt guest: `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.clean -n servo-ubuntu2204.new -f /dev/zvol/tank/base/servo-ubuntu2204.new`
- Update new base image: `./mount-runner.sh servo-ubuntu2204.new $PWD/ubuntu2204/configure-base.sh`
- Take another snapshot: `zfs snapshot tank/base/servo-ubuntu2204.new@configure-base`
- Boot temporary vm guest: `virsh start servo-ubuntu2204.new`
- Wait for the guest to shut down, which indicates Servo was built successfully
- Take another snapshot: `zfs snapshot tank/base/servo-ubuntu2204.new@ready`
- Destroy the old base image (if it exists): `zfs destroy -r tank/base/servo-ubuntu2204`
- Rename the new base image: `zfs rename tank/base/servo-ubuntu2204{.new,}`
- Undefine the temporary libvirt guest: `virsh undefine servo-ubuntu2204.new`
- Create the base libvirt guest (if it doesn’t exist): `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.clean -n servo-ubuntu2204 -f /dev/zvol/tank/base/servo-ubuntu2204`

To clone and start a new runner:

```sh
$ ./create-runner.sh servo-ubuntu2204 ready ubuntu2204
```

macOS 13 runner (wip)
---------------------

To build the base vm, first build a clean image:

- Clone the OSX-KVM repo: `git clone https://github.com/kholia/OSX-KVM.git /var/lib/libvirt/images/OSX-KVM`
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-macos13.clean`
    - `virsh define macos13.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-macos13.init -n servo-macos13.clean --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.servo-macos13.clean.fd -f /var/lib/libvirt/images/OSX-KVM/OpenCore/OpenCore.qcow2 -f /dev/zvol/tank/base/servo-macos13.clean -f /var/lib/libvirt/images/OSX-KVM/BaseSystem.img`
    - `cp /var/lib/libvirt/images/OSX-KVM/{OVMF_VARS-1920x1080.fd,OVMF_VARS.servo-macos13.clean.fd}`
    - `virsh undefine --keep-nvram servo-macos13.init`
        - TODO: improve per-vm nvram management
    - `virsh start servo-macos13.clean`
- Install macOS
    - At the boot menu, choose **macOS Base System**
    - **Utilities** > **Terminal**
        - `diskutil list | grep GB` and find the `diskN` line that is around 96.6 GB
        - `diskutil partitionDisk diskN 2 GPT  ExFAT a 60G  APFS macOS 0G`
    - Quit Terminal
    - **Reinstall macOS Ventura**
    - Shut down the guest when you see **Select Your Country or Region**: `virsh shutdown servo-macos13.clean`
- Take a snapshot: `zfs snapshot tank/base/servo-macos13.clean@fresh-install`
- Boot base vm guest: `virsh start servo-macos13.clean`
    - **Select Your Country or Region** = United States
    - **Migration Assistant** = Not Now
    - **Sign In with Your Apple ID** = Set Up Later
    - **Full name** = `servo`
    - **Account name** = `servo`
    - **Password** = `servo2024!`
    - **Enable Location Services** = Continue, Don’t Use
    - **Select Your Time Zone** > **Closest City:** = UTC - United Kingdom
    - Uncheck **Share Mac Analytics with Apple**
    - **Screen Time** = Set Up Later
    - Quit the **Keyboard Setup Assistant**
    - Once installed, shut down the guest: `virsh shutdown servo-macos13.clean`
- Take another snapshot: `zfs snapshot tank/base/servo-macos13.clean@oobe`

Baking new images after deployment
----------------------------------

- Restart the monitor with `SERVO_CI_DONT_CREATE_RUNNERS`, to free up some resources
- Update the Servo repo on the host: `git -C ~/servo pull`
- Redo the “build the base image” steps for the image to be built, stopping before the `zfs destroy` step
- Create a test profile in monitor/src/main.rs, pointing to the test image
- Restart the monitor without `SERVO_CI_DONT_CREATE_RUNNERS`
- Run a try job with that test image: `./mach try win -r upstream`
- If all goes well
    - Remove the test profile monitor/src/main.rs
    - Restart the monitor without `SERVO_CI_DONT_CREATE_RUNNERS`
    - Do the rest of the “build the base image” steps
