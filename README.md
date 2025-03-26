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

Current SSH host keys
---------------------

- ci0.servo.org = `SHA256:aoy+JW6hlkTwQDqdPZFY6/gDf1faOQGH5Zwft75Odrc` (ED25519)
- ci1.servo.org = `SHA256:ri52Ae31OABqL/xCss42cJd0n1qqhxDD9HvbOm59y8o` (ED25519)
- ci2.servo.org = `SHA256:qyetP4wIOHrzngj1SIpyEnAHJNttW+Rd1CzvJaf0x6M` (ED25519)

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

Start the [rescue system](https://docs.hetzner.com/robot/dedicated-server/troubleshooting/hetzner-rescue-system/), then run the following:

```
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ apt install -y zsh
$ ./start-nixos-installer.sh
```

Reconnect over SSH (use `ssh -4` this time), then run the following:

```
$ git clone https://github.com/servo/ci-runners.git
$ cd ci-runners/server
$ ./first-time-install.sh ci0 /dev/nvme{0,1}n1
$ reboot
```

To deploy an updated config to any of the servers:

```
$ cd server/nixos
$ ./deploy -s ci0.servo.org ci0
$ ./deploy -s ci1.servo.org ci1
$ ./deploy -s ci2.servo.org ci2
```

To deploy, read monitor config, write monitor config, or run a command on one or more servers:

```
$ cd server/nixos
$ ./do <deploy|read|write> [host ...]
$ ./do deploy ci0 ci1 ci2
$ ./do read ci0 ci1
$ ./do write ci1 ci2

$ ./do run [host ...] -- <command ...>
$ ./do run ci0 ci2 -- virsh edit servo-ubuntu2204
```

To monitor system logs or process activity on any of the servers:

```
$ ./do logs <host>
$ ./do htop <host>
```

Setting up the monitor service
------------------------------

To get a GITHUB_TOKEN for the monitor service in production:

- [Create](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
    - Token name: `servo ci monitor`
    - Expiration: **90 days**
    - Resource owner: **servo**
    - Repository access: **Public Repositories (read-only)**
    - Organization permissions > **Self-hosted runners** > Access: **Read and write**

To get a GITHUB_TOKEN for testing the monitor service:

- [Create](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
    - Token name: `servo ci monitor test`
    - Expiration: **7 days**
    - Resource owner: your GitHub account
    - Repository access > **Only select repositories** > your clone of servo/servo
    - Repository permissions > **Administration** > Access: **Read and write** (unfortunately there is no separate permission for repository self-hosted runners)

To set up the monitor service, run the following:

```
$ zfs create tank/base
$ zfs create tank/ci
$ git clone https://github.com/servo/ci-runners.git ~/ci-runners
$ cd ~/ci-runners
$ mkdir /var/lib/libvirt/images
$ virsh net-define cinet.xml
$ virsh net-autostart cinet
$ virsh net-start cinet

$ rustup default stable
$ mkdir ~/.cargo
$ git clone https://github.com/servo/servo.git ~/servo
$ mkdir /config /config/monitor
$ cp ~/ci-runners/.env.example /config/monitor/.env
$ cp ~/ci-runners/monitor/monitor.toml.example /config/monitor/monitor.toml
$ vim -p /config/monitor/.env /config/monitor/monitor.toml
$ systemctl restart monitor
```

Windows 10 runner
-----------------

Runners created from this image preinstall all dependencies (including those specified in the main repo, like GStreamer and Chocolatey deps), preload the main repo, and prebuild Servo in the release profile.

To prepare a server for Windows 10 guests:

- Download images into /var/lib/libvirt/images
    - Windows 10 (multi-edition ISO), English (United States): [Win10_22H2_English_x64v1.iso](https://www.microsoft.com/en-us/software-download/windows10ISO) (sha256 = a6f470ca6d331eb353b815c043e327a347f594f37ff525f17764738fe812852e)
    - Hint: grab the link, then `curl -Lo Win10_22H2_English_x64v1.iso '<link>'`

Building the base vm image is handled automatically by the monitor, with the help of `ubuntu2204/build-image.sh`.

Ubuntu runner
-------------

Runners created from this image preinstall all dependencies (including those specified in the main repo, like mach bootstrap deps), preload the main repo, and prebuild Servo in the release profile.

Building the base vm image is handled automatically by the monitor, with the help of `ubuntu2204/build-image.sh`.

macOS 13 runner
---------------

To prepare a server for macOS 13 guests, build a clean image:

- Clone the OSX-KVM repo: `git clone --recursive https://github.com/kholia/OSX-KVM.git /var/lib/libvirt/images/OSX-KVM`
- Download the BaseSystem.dmg for macOS Ventura: `( cd /var/lib/libvirt/images/OSX-KVM; ./fetch-macOS-v2.py )`
- Convert it to BaseSystem.img: `dmg2img -i /var/lib/libvirt/images/OSX-KVM/BaseSystem.{dmg,img}`
- Reduce the OpenCore `Timeout` setting:
    - `cd /var/lib/libvirt/images/OSX-KVM/OpenCore`
    - `vim config.plist`
    - Type `/<key>Timeout<`, press **Enter**, type `j0f>wcw5`, press **Escape**, type `:x`, press **Enter**
    - `rm OpenCore.qcow2`
    - `./opencore-image-ng.sh --cfg config.plist --img OpenCore.qcow2`
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-macos13.clean`
    - `virsh define macos13/guest.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-macos13.init -n servo-macos13.clean --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.servo-macos13.clean.fd --skip-copy sda -f /dev/zvol/tank/base/servo-macos13.clean --skip-copy sdc`
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
    - If latency is high:
        - Press **Command**+**Option**+**F5**, then click **Full Keyboard Access**, then press **Enter**
        - You can now press **Shift**+**Tab** to get to the buttons at the bottom of the wizard
    - **Select Your Country or Region** = United States
    - **Migration Assistant** = Not Now
    - If latency is high, **Accessibility** > **Vision** then:
        - \> **Reduce Transparency** = Reduce Transparency
        - \> **Reduce Motion** = Reduce Motion
    - **Sign In with Your Apple ID** = Set Up Later
    - **Full name** = `servo`
    - **Account name** = `servo`
    - **Password** = `servo2024!`
    - **Enable Location Services** = Continue, Don’t Use
    - **Select Your Time Zone** > **Closest City:** = UTC - United Kingdom
    - Uncheck **Share Mac Analytics with Apple**
    - **Screen Time** = Set Up Later
    - Quit the **Keyboard Setup Assistant**
    - If latency is high:
        - Press **Cmd**+**Space**, type `full keyboard access`, turn it on, then press **Cmd**+**Q**
    - Once installed, shut down the guest: `virsh shutdown servo-macos13.clean`
- When the guest shuts down, take another snapshot: `zfs snapshot tank/base/servo-macos13.clean@oobe`
- Start the base guest: `virsh start servo-macos13.clean`
- Log in with the password above
- Press **Cmd**+**Space**, type `full disk access`, press **Enter**
- Click the plus, type the password above, type `/System/Applications/Utilities/Terminal.app`, press **Enter** twice, press **Cmd**+**Q**
- Press **Cmd**+**Space**, type `terminal`, press **Enter**
- Type `curl https://ci0.servo.org/static/macos13.sh | sudo sh`, press **Enter**, type the password above, press **Enter**
- When the guest shuts down, take another snapshot: `zfs snapshot tank/base/servo-macos13.clean@automated`
- Enable per-snapshot block devices for the zvol: `zfs set snapdev=visible tank/base/servo-macos13.clean`

Building the base vm image is handled automatically by the monitor, with the help of `macos13/build-image.sh`.
