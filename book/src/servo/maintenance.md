# Maintenance guide

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
