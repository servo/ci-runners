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
$ /config/server/update.sh /config/server/nixos
```

To set up libvirt:

- Connect via virt-manager on another machine
    - File > Add Connection…
        - \> Hypervisor: QEMU/KVM
        - \> Connect to remote host over SSH
        - \> Username: root
        - \> Hostname: ci0.servo.org
        - \> Connect
- Configure and start the “default” network for NAT
    - Edit > Connection Details > Virtual Networks > default
        - \> Autostart: On Boot
        - \> Start Network

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
$ rustup default stable
$ zfs create tank/base
$ zfs create tank/ci
$ git clone https://github.com/servo/servo.git ~/servo
$ cp /config/.env.example /config/.env
$ vim /config/.env
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
    - `virsh start servo-windows10.clean`
- Install Windows 10 Pro
    - Click “I don't have a product key”
    - Load disk driver from `E:\vioscsi\w10\amd64`
    - Shut down the guest when you see “Let’s start with region. Is this right?”: `virsh shutdown servo-windows10.clean`
- Take a snapshot: `zfs snapshot tank/base/servo-windows10.clean@fresh-install`
- Boot base vm guest: `virsh start servo-windows10.clean`
    - Click “I don’t have internet”
    - Click “Continue with limited setup”
    - Set username to `servo`
    - Leave password empty
    - Turn off the six privacy settings
    - Click “Not now” for Cortana
    - Once installed, shut down the guest: `shutdown /s /t 0`
- Take another snapshot: `zfs snapshot tank/base/servo-windows10.clean@oobe`

Then build the base image:

- Clone the clean image: `zfs clone tank/base/servo-windows10{.clean@oobe,.new}`
- Create a temporary libvirt guest: `virt-clone --preserve-data --check path_in_use=off -o servo-windows10.clean -n servo-windows10.new -f /dev/zvol/tank/base/servo-windows10.new`
- Update new base image: `./mount-runner.sh servo-windows10.new $PWD/windows10/configure-base.sh`
- Take a snapshot: `zfs snapshot tank/base/servo-windows10.new@configure-base`
- Boot temporary vm guest: `virsh start servo-windows10.new`
    - Open an elevated PowerShell: **Win**+**X**, **A**
    - Allow running scripts: `Set-ExecutionPolicy -ExecutionPolicy Unrestricted -Force`
    - Run the init script once: `C:\init\init.ps1`
    - Once installed, shut down the guest: `shutdown /s /t 0`
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
    - Ubuntu Server 22.04.4: [ubuntu-22.04.4-live-server-amd64.iso](http://mirror.internode.on.net/pub/ubuntu/releases/22.04.4/ubuntu-22.04.4-live-server-amd64.iso) (sha256 = 45f873de9f8cb637345d6e66a583762730bbea30277ef7b32c9c3bd6700a32b2)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-ubuntu2204.clean`
    - `virsh define ubuntu2204.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.init -n servo-ubuntu2204.clean -f /dev/zvol/tank/base/servo-ubuntu2204.clean`
    - `virsh undefine servo-ubuntu2204.init`
    - `virsh start servo-ubuntu2204.clean`
- Install Ubuntu
    - Uncheck “Set up this disk as an LVM group”
    - Use hostname `servo-ubuntu2204`
    - Use credentials `servo` and `servo2024!`
    - If we want SSH access for debugging…
        - Check “Install OpenSSH server”
        - Provide a SSH public key
        - Uncheck “Allow password authentication over SSH”
    - …otherwise, uncheck “Install OpenSSH server”
    - Once installed, choose “Reboot now”
    - Press Enter when prompted about the installation medium (no need to eject)
    - Once rebooted, shut down the guest
- Take a snapshot: `zfs snapshot tank/base/servo-ubuntu2204.clean@fresh-install`

Then build the base image:

- Clone the clean image: `zfs clone tank/base/servo-ubuntu2204{.clean@fresh-install,.new}`
- Create a temporary libvirt guest: `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.clean -n servo-ubuntu2204.new -f /dev/zvol/tank/base/servo-ubuntu2204.new`
- Update new base image: `./mount-runner.sh servo-ubuntu2204.new $PWD/ubuntu2204/configure-base.sh`
- Take another snapshot: `zfs snapshot tank/base/servo-ubuntu2204.new@configure-base`
- Boot temporary vm guest: `virsh start servo-ubuntu2204.new`
    - Once installed, log in and check that rc.local succeeded: `journalctl -b`
    - If the init script succeeded, shut down the guest
- Take another snapshot: `zfs snapshot tank/base/servo-ubuntu2204.new@ready`
- Destroy the old base image (if it exists): `zfs destroy -r tank/base/servo-ubuntu2204`
- Rename the new base image: `zfs rename tank/base/servo-ubuntu2204{.new,}`
- Undefine the temporary libvirt guest: `virsh undefine servo-ubuntu2204.new`
- Create the base libvirt guest (if it doesn’t exist): `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204.clean -n servo-ubuntu2204 -f /dev/zvol/tank/base/servo-ubuntu2204`

To clone and start a new runner:

```sh
$ ./create-runner.sh servo-ubuntu2204 ready ubuntu2204
```

Baking new images after deployment
----------------------------------

- Restart the monitor with `SERVO_CI_DONT_CREATE_RUNNERS`, to free up some resources
- Update the Servo repo on the host: `git -C ~/servo pull`
- Redo the “build the base image” steps for the image to be built
    - `zfs destroy -r tank/base/servo-windows10` will fail until there are no busy runners on the old image
- Restart the monitor without `SERVO_CI_DONT_CREATE_RUNNERS`, to free up some resources
