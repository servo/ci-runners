#!/usr/bin/env bash
# A small script to send an email when a service fails.
# Usage: servo_ci_runner_send_mail.sh <service_name>
# This script is intended to be used with systemd's OnFailure directive.
# Before using this script, ensure that you have configured your email settings correctly in /etc/ssmtp/ssmtp.conf.
# Make sure to replace XXXinsert your email address hereXXX with your actual email address in the script.
# You may setup multiple receivers.

set -eu
service_name="$1"
host_name=$(hostname)
{
  echo "To: XXXinsert your email address hereXXX"
  echo "From: DRC servo CI <XXXinsert your email address hereXXX>"
  echo "Subject: Service ${service_name} failed on Host ${host_name}"
  echo 'Content-Type: text/plain; charset="UTF-8"'
  echo
  echo "Service ${service_name} failed on Host ${host_name}!"
  echo "Systemctl status:"
  echo "\`\`\`"
  echo "$(systemctl --user status ${service_name})"
  echo "\`\`\`"
  echo "Service logs of last 5 minutes:"
  echo "\`\`\`"
  echo "$(journalctl --user -u servo_ci --since '5 minutes ago')"
  echo "\`\`\`"
  echo "Disk usage:"
  echo "\`\`\`"
  echo "$(df -h)"
  echo "\`\`\`"
} | tee /tmp/servo_ci_mail_dump.txt | /usr/bin/env ssmtp XXXinsert your email address hereXXX
