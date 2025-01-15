#!/bin/sh
set -eu

# Enable SSH
systemsetup -setremotelogin on

# Enable automatic login
curl -fsSO https://ci0.servo.org/static/macos13/setAutoLogin.sh
chmod +x setAutoLogin.sh
./setAutoLogin.sh servo 'servo2024!'

# Allow servo to elevate to root without password
echo 'servo  ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/servo

# Install a LaunchAgent to run our code on boot
# <https://superuser.com/a/229792>
curl -fsSO https://ci0.servo.org/static/macos13/org.servo.ci.plist
mv -v org.servo.ci.plist /Library/LaunchAgents

# Shut down the clean image guest
shutdown -h now
