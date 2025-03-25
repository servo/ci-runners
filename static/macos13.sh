#!/usr/bin/env zsh
set -eu
macos_version=13
mkdir -p /var/root/ci
cd /var/root/ci

# Enable SSH
systemsetup -setremotelogin on

# Enable automatic login
(
    mkdir -p autologin
    cd autologin
    curl -fsSO https://ci0.servo.org/static/macos13/setAutoLogin.sh
    chmod +x setAutoLogin.sh
    ./setAutoLogin.sh servo 'servo2024!'
)

# Allow servo to elevate to root without password
echo 'servo  ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/servo

# Install a LaunchAgent to run our code on boot
# <https://superuser.com/a/229792>
(
    mkdir -p launchd
    cd launchd
    curl -fsSO https://ci0.servo.org/static/macos13/org.servo.ci.plist
    mv -v org.servo.ci.plist /Library/LaunchAgents
)

# Disable the Terminal.app session restore feature to avoid sketchy “command not found” errors
# - Method based on <https://apple.stackexchange.com/a/347045>
# - Another possible method (2018) <https://superuser.com/a/1303096>
# - Another method that doesn’t seem to work (2020) <https://superuser.com/a/1610999>
# - More about the errors <https://apple.stackexchange.com/q/465930>
# - More about the feature <https://apple.stackexchange.com/q/278372>
# - Possibly related feature <https://superuser.com/q/1293690>
find /Users/servo/Library/Saved\ Application\ State/com.apple.Terminal.savedState -depth +0 -delete || mkdir /Users/servo/Library/Saved\ Application\ State/com.apple.Terminal.savedState
chflags uchg /Users/servo/Library/Saved\ Application\ State/com.apple.Terminal.savedState

# Install Xcode CLT (Command Line Tools) non-interactively
# <https://github.com/actions/runner-images/blob/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh>
if ! xcode-select -p; then (
    mkdir -p xcode
    cd xcode
    mkdir -p utils
    touch utils/utils.sh
    curl -LO https://raw.githubusercontent.com/actions/runner-images/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh
    chmod +x install-xcode-clt.sh
    ./install-xcode-clt.sh
); fi

# Install Homebrew
if ! [ -e /usr/local/bin/brew ]; then (
    mkdir -p homebrew
    cd homebrew
    sudo -iu servo curl -LO https://raw.githubusercontent.com/Homebrew/install/9a01f1f361cc66159c31624df04b6772d26b7f98/install.sh
    sudo -iu servo chmod +x install.sh
    sudo -iu servo NONINTERACTIVE=1 ./install.sh
); fi

# Compile and install ntfs-3g
if ! [ -e /usr/local/sbin/mkntfs ]; then (
    mkdir -p ntfs
    cd ntfs

    # <https://github.com/tuxera/ntfs-3g/issues/130>
    sudo -iu servo brew install autoconf automake m4 libtool pkg-config libgcrypt macfuse

    curl -LO https://github.com/tuxera/ntfs-3g/archive/refs/tags/2022.10.3.tar.gz
    rm -Rf ntfs-3g-2022.10.3
    tar xf 2022.10.3.tar.gz
    cd ntfs-3g-2022.10.3
    # error: required file './ltmain.sh' not found
    autoreconf -fi || autoreconf -fi
    # <https://github.com/tuxera/ntfs-3g/issues/5>
    # <https://gist.github.com/six519/9f04837f119103d4ff45542a5b5d4222>
    LDFLAGS="-L/usr/local/lib -lintl" ./configure --exec-prefix=/usr/local
    make -j
    make install
); fi

# Convert /Volumes/a from exFAT to NTFS
# First disable Spotlight so we can unmount (per `lsof +f -- /Volumes/a`)
# <https://apple.stackexchange.com/a/444826>
mdutil -d /Volumes/a || :
# Check if mounted, but not by checking for existence of the mount point
if mount | grep -q ' on /Volumes/a '; then
    umount /Volumes/a
fi
/usr/local/sbin/mkntfs --quick --label a /dev/disk2s2

# Write out a boot script that mounts /Volumes/a and runs /Volumes/a/init.sh
cat > /Users/servo/boot.sh <<'END'
#!/usr/bin/env zsh
set -eu
# Unmount the volume first, in case it was mounted with the built-in read-only NTFS driver
if mount | grep -q ' on /Volumes/a '; then
    sudo umount /Volumes/a
fi
sudo mkdir -p /Volumes/a
# Mount the volume as servo:staff to avoid permission errors
sudo ntfs-3g -o uid=501,gid=20 /dev/disk2s2 /Volumes/a
if [ -e /Volumes/a/init/init.sh ]; then
    /Volumes/a/init/init.sh
else
    echo /Volumes/a/init/init.sh does not exist
fi
END
chmod +x /Users/servo/boot.sh

# <https://github.com/actions/runner-images/issues/4731>
kextload /Library/Filesystems/macfuse.fs/Contents/Extensions/$macos_version/macfuse.kext || :

echo
echo
echo 'See the README for next steps'
