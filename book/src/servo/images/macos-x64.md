# macOS 13/14/15 x64 images

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
