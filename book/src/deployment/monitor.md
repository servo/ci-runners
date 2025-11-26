# Setting up the monitor service

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
    - Repository access > **Only select repositories**
        - > Your clone of servo/ci-runners
        - > Your clone of servo/servo
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
