GitHub Actions runners for Servo
================================

Windows Server 2019 runner
--------------------------

Runners created from this image preinstall all dependencies (including those specified in the main repo, like GStreamer and Chocolatey deps), preload the main repo, and prebuild Servo in the release profile.

To build the base vm:

- Download images into /var/lib/libvirt/images
    - Windows Server 2019: [17763.3650.221105-1748.rs5_release_svc_refresh_SERVER_EVAL_x64FRE_en-us.iso](https://software-static.download.prss.microsoft.com/dbazure/988969d5-f34g-4e03-ac9d-1f9786c66749/17763.3650.221105-1748.rs5_release_svc_refresh_SERVER_EVAL_x64FRE_en-us.iso) (sha256 = 6dae072e7f78f4ccab74a45341de0d6e2d45c39be25f1f5920a2ab4f51d7bcbb)
    - VirtIO drivers: [virtio-win-0.1.240.iso](https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.240-1/virtio-win-0.1.240.iso) (sha256 = ebd48258668f7f78e026ed276c28a9d19d83e020ffa080ad69910dc86bbcbcc6)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G mypool/servo-windows2019`
    - `virsh define windows2019.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-windows2019-init -n servo-windows2019 -f /dev/zvol/mypool/servo-windows2019`
    - `virsh undefine servo-windows2019-init`
- Install Windows Server with desktop experience
    - Core can build Servo, but trying to run it yields DeviceOpenFailed in surfman
    - Load disk driver from `E:\vioscsi\2k19\amd64`
    - Set password for Administrator to `servo2024!`
    - Once installed, shut down the guest: `shutdown /s /t 0`
- Take a snapshot: `zfs snapshot mypool/servo-windows2019@0-fresh-install`
- Update base vm image: `./mount-runner.sh servo-windows2019 $PWD/windows2019/configure-base.sh`
- Take another snapshot: `zfs snapshot mypool/servo-windows2019@1-configure-base`
- Boot base vm guest: `virsh start servo-windows2019`
    - The guest will reboot twice, due to the .NET and MSVC installations
    - Once installed, shut down the guest: `shutdown /s /t 0`
- Take another snapshot: `zfs snapshot mypool/servo-windows2019@2-ready`

To clone and start a new runner:

```sh
$ ./create-runner.sh servo-windows2019 2-ready $PWD/windows2019/configure-runner.sh sudo -iu delan $PWD/register-runner.sh '..\a' Windows
```

Windows 10 runner
-----------------

To build the base vm:

- Download images into /var/lib/libvirt/images
    - Windows 10 (multi-edition ISO), English (United States): [Win10_22H2_English_x64v1.iso](https://www.microsoft.com/en-us/software-download/windows10ISO) (sha256 = a6f470ca6d331eb353b815c043e327a347f594f37ff525f17764738fe812852e)
    - VirtIO drivers: [virtio-win-0.1.240.iso](https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.240-1/virtio-win-0.1.240.iso) (sha256 = ebd48258668f7f78e026ed276c28a9d19d83e020ffa080ad69910dc86bbcbcc6)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G mypool/servo-windows10`
    - `virsh define windows10.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-windows10-init -n servo-windows10 -f /dev/zvol/mypool/servo-windows10`
    - `virsh undefine servo-windows10-init`
- Install Windows 10 Pro
    - Click “I don't have a product key”
    - Load disk driver from `E:\vioscsi\w10\amd64`
    - Shut down the guest when you see “Let’s start with region. Is this right?”: `virsh shutdown servo-windows10`
- Take a snapshot: `zfs snapshot mypool/servo-windows10@0-fresh-install`
- Boot base vm guest: `virsh start servo-windows10`
    - Click “I don’t have internet”
    - Click “Continue with limited setup”
    - Set username to `servo`
    - Leave password empty
    - Turn off the six privacy settings
    - Click “Not now” for Cortana
    - Once installed, shut down the guest: `shutdown /s /t 0`
- Take another snapshot: `zfs snapshot mypool/servo-windows10@1-oobe`
- Update base vm image: `./mount-runner.sh servo-windows10 $PWD/windows2019/configure-base.sh`
- Take another snapshot: `zfs snapshot mypool/servo-windows10@2-configure-base`
- Boot base vm guest: `virsh start servo-windows10`
    - Open an elevated PowerShell: **Win**+**X**, **A**
    - Allow running scripts: `Set-ExecutionPolicy -ExecutionPolicy Unrestricted -Force`
    - Run the init script once: `C:\init\init.ps1`

Ubuntu runner
-------------

To build the base vm:

- Download images into /var/lib/libvirt/images
    - Ubuntu Server 22.04.4: [ubuntu-22.04.4-live-server-amd64.iso](http://mirror.internode.on.net/pub/ubuntu/releases/22.04.4/ubuntu-22.04.4-live-server-amd64.iso)
- Create zvol and libvirt guest with random UUID and MAC address
    - `zfs create -V 90G mypool/servo-ubuntu2204`
    - `virsh define ubuntu2204.xml`
    - `virt-clone --preserve-data --check path_in_use=off -o servo-ubuntu2204-init -n servo-ubuntu2204 -f /dev/zvol/mypool/servo-ubuntu2204`
    - `virsh undefine servo-ubuntu2204-init`
- Install Ubuntu
    - Uncheck “Set up this disk as an LVM group”
    - Use hostname `servo-ubuntu2204`
    - Use credentials `servo` and `servo2024!`
    - Check “Install OpenSSH server”
    - Uncheck “Allow password authentication over SSH”
    - Once installed, shut down the guest
- Take a snapshot: `zfs snapshot mypool/servo-ubuntu2204@0-fresh-install`
- Update base vm image: `./mount-runner.sh servo-ubuntu2204 $PWD/ubuntu2204/configure-base.sh`
- Take another snapshot: `zfs snapshot mypool/servo-ubuntu2204@1-configure-base`
- Boot base vm guest: `virsh start servo-ubuntu2204`
    - Once installed, shut down the guest
- Take another snapshot: `zfs snapshot mypool/servo-ubuntu2204@2-ready`

To clone and start a new runner:

```sh
$ ./create-runner.sh servo-ubuntu2204 2-ready $PWD/ubuntu2204/configure-runner.sh sudo -iu delan $PWD/register-runner.sh ../a Linux
```
