GitHub Actions runners for Servo
================================

This repo contains:

- Server config and install scripts
    - `server/nixos` is the NixOS config
- Templates for CI runner images
    - `profiles/servo-windows10/*` is for **Windows 10** runners
    - `profiles/servo-ubuntu2204/*` is for **Ubuntu 22.04** runners
    - `profiles/servo-macos13/*` is for **macOS 13** runners
    - `profiles/servo-macos14/*` is for **macOS 14** runners
    - `profiles/servo-macos15/*` is for **macOS 15** runners
- A service that automates runner management
    - `monitor` is the service
    - `.env.example` and `monitor.toml.example` contain the settings

Maintenance guide
-----------------

Current SSH host keys:

- ci0.servo.org = `SHA256:aoy+JW6hlkTwQDqdPZFY6/gDf1faOQGH5Zwft75Odrc` (ED25519)
- ci1.servo.org = `SHA256:ri52Ae31OABqL/xCss42cJd0n1qqhxDD9HvbOm59y8o` (ED25519)
- ci2.servo.org = `SHA256:qyetP4wIOHrzngj1SIpyEnAHJNttW+Rd1CzvJaf0x6M` (ED25519)
- ci3.servo.org = `SHA256:4grnt9EVzUhnRm7GR5wR1vwEMXkMHx+XCYkns6WfA9s` (ED25519)
- ci4.servo.org = `SHA256:Yc1TdE2UDyG2wUUE0uGHoWwbbvUkb1i850Yye9BC0EI` (ED25519)

To deploy an updated config to any of the servers:

```
$ cd server/nixos
$ ./deploy -s ci0.servo.org ci0
$ ./deploy -s ci1.servo.org ci1
$ ./deploy -s ci2.servo.org ci2
$ ./deploy -s ci3.servo.org ci3
$ ./deploy -s ci4.servo.org ci4
```

To deploy, read monitor config, write monitor config, restart the monitor, or run a command on one or more servers:

```
$ cd server/nixos
$ ./do <deploy|read|write> [host ...]
$ ./do deploy ci0 ci1 ci2
$ ./do read ci0 ci1
$ ./do write ci1 ci2
$ ./do restart-monitor ci0 ci1 ci2

$ ./do run [host ...] -- <command ...>
$ ./do run ci0 ci2 -- virsh edit servo-ubuntu2204
```

To monitor system logs or process activity on any of the servers:

```
$ ./do logs <host>
$ ./do htop <host>
```

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

Setting up the monitor service
------------------------------

To get a GITHUB_TOKEN for the monitor service in production:

- [Create](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
    - Token name: `servo ci monitor`
    - Resource owner: **servo**
    - Expiration: **90 days**
    - Repository access: **Public Repositories (read-only)**
    - Organization permissions > **Self-hosted runners** > Access: **Read and write**

To get a GITHUB_TOKEN for testing the monitor service:

- [Create](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)
    - Token name: `servo ci monitor test`
    - Resource owner: your GitHub account
    - Expiration: **7 days**
    - Repository access > **Only select repositories** > your clone of servo/servo
    - Repository permissions > **Administration** > Access: **Read and write** (unfortunately there is no separate permission for repository self-hosted runners)

To set up the monitor service, connect over SSH (`mosh` recommended) and run the following:

```
$ zfs create tank/base
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

Hacking on the monitor locally
------------------------------

Easy but slow way:

```
$ nix develop -c sudo [RUST_BACKTRACE=1] monitor
```

Harder but faster way:

```
$ export RUSTFLAGS=-Clink-arg=-fuse-ld=mold
$ cargo build
$ sudo [RUST_BACKTRACE=1] IMAGE_DEPS_DIR=$(nix build --print-out-paths .\#image-deps) LIB_MONITOR_DIR=. target/debug/monitor
```

Minimal test images
-------------------

A useful image for hacking on the monitor locally, since it only takes around a minute to build.

This is a **libvirt/KVM**-based image, compatible with Linux amd64 servers only:

- `servo-ubuntu2204-rust`

Windows 10 images
-----------------

Runners created from these images preinstall all dependencies (including those specified in the main repo, like GStreamer and Chocolatey deps), preload the main repo, and prebuild Servo in the release profile.

This is a **libvirt/KVM**-based image, compatible with Linux amd64 servers only:

- `servo-windows10`

Ubuntu 22.04 images
-------------------

Runners created from these images preinstall all dependencies (including those specified in the main repo, like mach bootstrap deps), preload the main repo, and prebuild Servo in the release profile.

These are **libvirt/KVM**-based images, compatible with Linux amd64 servers only:

- `servo-ubuntu2204` (ci0, ci1, and ci2 only)
- `servo-ubuntu2204-bench` (ci3 and ci4 only)

macOS 13/14/15 x64 images
-------------------------

Runners created from these images preinstall all dependencies (including those specified in the main repo, like mach bootstrap deps), preload the main repo, and prebuild Servo in the release profile.

These are **libvirt/KVM**-based images, compatible with Linux amd64 servers only:

- `servo-macos13`
- `servo-macos14`
- `servo-macos15`

Automating the macOS installer is difficult without paid tooling, but we can get close enough with some once-per-server setup. To prepare a server for macOS 13/14/15 guests, build a clean image, replacing “13” with the macOS version as needed:

- Clone the OSX-KVM repo: `git clone --recursive https://github.com/kholia/OSX-KVM.git /var/lib/libvirt/images/OSX-KVM`
- Download the BaseSystem.dmg: `( cd /var/lib/libvirt/images/OSX-KVM; ./fetch-macOS-v2.py )`
- Rename it to reflect the macOS version: `mv /var/lib/libvirt/images/OSX-KVM/BaseSystem{,.macos13}.dmg`
- Convert that .dmg to .img: `dmg2img -i /var/lib/libvirt/images/OSX-KVM/BaseSystem.macos13.{dmg,img}`
- Reduce the OpenCore `Timeout` setting:
    - `cd /var/lib/libvirt/images/OSX-KVM/OpenCore`
    - `vim config.plist`
    - Type `/<key>Timeout<`, press **Enter**, type `j0f>wcw5`, press **Escape**, type `:x`, press **Enter**
    - `rm OpenCore.qcow2`
    - `./opencore-image-ng.sh --cfg config.plist --img OpenCore.qcow2`
    - `cp /var/lib/libvirt/images/OSX-KVM/OpenCore/OpenCore{,.macos13}.qcow2`
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G tank/base/servo-macos13.clean`
    - `virsh define profiles/servo-macos13/guest.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-macos13.init -n servo-macos13.clean --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.servo-macos13.clean.fd --skip-copy sda -f /dev/zvol/tank/base/servo-macos13.clean --skip-copy sdc`
    - `cp /var/lib/libvirt/images/OSX-KVM/{OVMF_VARS-1920x1080.fd,OVMF_VARS.servo-macos13.clean.fd}`
    - `virsh undefine --keep-nvram servo-macos13.init`
        - TODO: improve per-vm nvram management
    - `virsh start servo-macos13.clean`
- Install macOS
    - At the boot menu, choose **macOS Base System**
    - Choose **Disk Utility**
    - Choose the **QEMU HARDDISK Media** listed as **Uninitialized**
    - Click **Erase**, click **Erase**, then click **Done**
    - Press **Cmd**+**Q** to quit Disk Utility
    - macOS 13: Choose **Reinstall macOS Ventura**
    - macOS 14: Choose **Reinstall macOS Sonoma**
    - macOS 15: Choose **Reinstall macOS Sequoia**
    - When asked to select a disk, choose **Untitled**
    - Shut down the guest when you see **Select Your Country or Region**: `virsh shutdown servo-macos13.clean`
- Take a snapshot: `zfs snapshot tank/base/servo-macos13.clean@fresh-install`
- Boot base vm guest: `virsh start servo-macos13.clean`
    - If latency is high:
        - Press **Command**+**Option**+**F5**, then click **Full Keyboard Access**, then press **Enter**
        - You can now press **Shift**+**Tab** to get to the buttons at the bottom of the wizard
    - **Select Your Country or Region** = United States
    - If latency is high, **Accessibility** > **Vision** then:
        - \> **Reduce Transparency** = Reduce Transparency
        - \> **Reduce Motion** = Reduce Motion
    - TODO: macOS 15: do we need to uncheck the box for allowing password reset via Apple ID?
    - macOS 13/14: **Migration Assistant** = Not Now
    - macOS 15: **Transfer Your Data to This Mac** = Set up as new
    - macOS 13/14: **Sign In with Your Apple ID** = Set Up Later
    - macOS 15: **Sign In to Your Apple Account** = Set Up Later
    - **Full name** = `servo`
    - **Account name** = `servo`
    - **Password** = `servo2024!`
    - **Enable Location Services** = Continue, Don’t Use
    - **Select Your Time Zone** > **Closest City:** = UTC - United Kingdom
    - Uncheck **Share Mac Analytics with Apple**
    - **Screen Time** = Set Up Later
    - macOS 15: **Update Mac Automatically** = Only Download Automatically
        - TODO: can we prevent the download too?
    - Quit the **Keyboard Setup Assistant**
    - If latency is high:
        - Press **Cmd**+**Space**, type `full keyboard access`, turn it on, then press **Cmd**+**Q**
        - On macOS 15, this may make some steps *harder* to do with keyboard navigation for some reason
    - Once installed, shut down the guest: `virsh shutdown servo-macos13.clean`
- When the guest shuts down, take another snapshot: `zfs snapshot tank/base/servo-macos13.clean@oobe`
- Start the base guest: `virsh start servo-macos13.clean`
- Log in with the password above
- Press **Cmd**+**Space**, type `full disk access`, press **Enter**
    - On macOS 14/15, you may have to explicitly select **Allow applications to access all user files**
- Click the plus, type the password above, type `/System/Applications/Utilities/Terminal.app`, press **Enter** twice, press **Cmd**+**Q**
- Press **Cmd**+**Space**, type `terminal`, press **Enter**
- Type `curl https://ci0.servo.org/static/macos13.sh | sudo sh`, press **Enter**, type the password above, press **Enter**
- When the guest shuts down, take another snapshot: `zfs snapshot tank/base/servo-macos13.clean@automated`
- Copy the clean image to a file: `dd status=progress iflag=fullblock bs=1M if=/dev/zvol/tank/base/servo-macos13.clean of=/var/lib/libvirt/images/servo-macos13.clean.img`

Remote deployment tip. If you’ve deployed the clean image, but now the base image rebuilds are getting stuck at the macOS installer menu, your NVRAM may not be set to boot from the correct disk. You can work around this by nulling out the BaseSystem.dmg disk in the clean guest config:

- Edit the clean guest: `virsh edit servo-macos13.clean`
- Find the `<disk>` block containing `sdc` and `BaseSystem`
- Change `<disk type="file" ...>` to `<disk type="block" ...>`
- Change `<source file="..."/>` to `<source dev="/dev/null"/>`
- Save and quit (nano): Ctrl+X, Y, Enter
- Restart the monitor: `systemctl restart monitor`

macOS 15 arm64 images
---------------------

Runners created from these images preinstall all dependencies (including those specified in the main repo, like mach bootstrap deps), preload the main repo, and prebuild Servo in the release profile.

This is a **UTM**-based image, compatible with macOS arm64 servers only:

- `servo-macos15-arm`

To prepare a macOS server for macOS guests and build a clean image, replacing “15” with the macOS version as needed:

- Install [Rust](https://rustup.rs)
- Install [Nix](https://nixos.org/download/#nix-install-macos) (the package manager)
- Install [UTM for macOS](https://mac.getutm.app) — the GitHub download version is free of charge
- Install [Homebrew](https://brew.sh) and libvirt:
    - `brew install libvirt`
    - `brew services start libvirt`
    - TODO: make libvirt optional and remove this?
- Clone and enter this repo:
    - `git clone https://github.com/servo/ci-runners.git ~/ci-runners`
    - `cd ~/ci-runners`
- Configure the monitor:
    - `cp .env.example .env`
    - `cp monitor.toml.example monitor.toml`
    - `vim .env`
        - **LIBVIRT_DEFAULT_URI** = `qemu:///session`
    - `vim monitor.toml.example`
        - **available_1g_hugepages** = **0** (it’s Linux only)
        - **available_normal_memory** = RAM minus 8G
        - Zero out the **target_count** of any Linux-only profiles
- Download the IPSW list for macOS 15:
    - macOS 15: `curl -fsSo com_apple_macOSIPSW_20250825041725.xml --compressed https://web.archive.org/web/20250825041725id_/https://mesu.apple.com/assets/macos/com_apple_macOSIPSW/com_apple_macOSIPSW.xml`
- Download the IPSW image for macOS 15:
    - macOS 15: `curl -O $(< com_apple_macOSIPSW_20250825041725.xml sed '/>VirtualMac2,1</,/>https:/!d' | tail -1 | egrep -o 'https://[^<]+')`
- In UTM:
    - **Settings** > **Network** > **Regenerate MAC addresses on clone** = on
    - Close the settings window
    - **Create a New Virtual Machine** > **Virtualize** > **macOS 12+**
    - **Memory** = **8192** MiB (TODO: update for prod environment)
    - **CPU Cores** = **5** (TODO: update for prod environment)
    - On the **macOS** page: **Import IPSW** > **Browse...** > `~/ci-runners/UniversalMac_15.6.1_24G90_Restore.ipsw`
    - On the **Storage** page: **Size** = **90** GiB (TODO: update for prod environment)
    - On the **Summary** page:
        - **Name** = **servo-macos15-arm-clean**
        - **Open VM Settings** = on
    - In the VM settings:
        - **Virtualization** > **Enable Sound** = off
        - **Virtualization** > **Enable Clipboard Sharing** = off
        - **Display** > **Resolution** = 1280 × 800 (this should be enough for a [WPT reftest](https://web-platform-tests.org/reviewing-tests/checklist.html#reftests-only))
        - **Display** > **Dynamic Resolution** = off
    - Start **servo-macos15-arm-clean** using the GUI
        - There seems to be a bug in UTM 4.7.4 (115) that breaks the VM if you start it with AppleScript
    - At the **Confirmation** prompt: **Would you like to install macOS?** > **OK**
- Once the clean vm boots:
    - **Language** = **English**
    - If latency is high:
        - FIXME: this seems to be busted in Remmina + UTM + macOS 15 (Command+Option+F5 just enables VoiceOver)
        - Press **Command**+**Option**+**F5**, then click **Full Keyboard Access**, then press **Enter**
        - You can now press **Shift**+**Tab** to get to the buttons at the bottom of the wizard
    - **Select Your Country or Region** = United States
    - macOS 15: **Transfer Your Data to This Mac** = Set up as new
    - If latency is high, **Accessibility** > **Vision** then:
        - \> **Reduce Transparency** = Reduce Transparency
        - \> **Reduce Motion** = Reduce Motion
    - **Full name** = `servo`
    - **Account name** = `servo`
    - **Password** = `servo2024!`
    - macOS 15: **Allow computer account password to be reset with your Apple Account** = off
    - macOS 15: **Sign In to Your Apple Account** = Set Up Later
    - **Enable Location Services** = Continue, Don’t Use
    - **Select Your Time Zone** > **Closest City:** = UTC - United Kingdom
    - Uncheck **Share Mac Analytics with Apple**
    - **Screen Time** = Set Up Later
    - macOS 15: **Update Mac Automatically** = Only Download Automatically
        - TODO: can we prevent the download too?
    - Quit the **Keyboard Setup Assistant**
    - If latency is high:
        - Press **Cmd**+**Space**, type `full keyboard access`, turn it on, then press **Cmd**+**Q**
        - On macOS 15, this may make some steps *harder* to do with keyboard navigation for some reason
    - Once installed, shut down macOS:
        - **Apple logo** > **Shut Down…**
        - **Reopen windows when logging back in** = off
        - **Shut Down**
- When the guest shuts down, clone it: `osascript -e 'tell application "UTM"' -e 'set vm to virtual machine named "servo-macos15-arm-clean"' -e 'duplicate vm with properties {configuration: {name: "servo-macos15-arm-clean@oobe"}}' -e 'end tell'`
    - You may be prompted for permission in the GUI
- Start the clean vm (the original, not the clone): `osascript -e 'tell application "UTM"' -e 'set vm to virtual machine named "servo-macos15-arm-clean"' -e 'start vm' -e 'end tell'`
- Log in with the password above
- In the UTM title bar, click **Capture input devices**
- Press **Cmd**+**Space**, type `full disk access`, press **Enter**
    - On macOS 14/15, you may have to explicitly select **Allow applications to access all user files**
- Click the plus, type the password above, type `/System/Applications/Utilities/Terminal.app`, press **Enter** twice, press **Cmd**+**Q**
- Shut down macOS: **Apple logo** > **Shut down…**
- When the guest shuts down, make another clone: `osascript -e 'tell application "UTM"' -e 'set vm to virtual machine named "servo-macos15-arm-clean"' -e 'duplicate vm with properties {configuration: {name: "servo-macos15-arm-clean@preautomated"}}' -e 'end tell'`
- Press **Cmd**+**Space**, type `terminal`, press **Enter**
- Type `curl https://ci0.servo.org/static/macos13.sh | sudo sh`, press **Enter**, type the password above, press **Enter**
- When the guest shuts down, make another clone: `osascript -e 'tell application "UTM"' -e 'set vm to virtual machine named "servo-macos15-arm-clean"' -e 'duplicate vm with properties {configuration: {name: "servo-macos15-arm-clean@automated"}}' -e 'end tell'`
