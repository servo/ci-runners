# Based on https://github.com/TermonyHQ/Termony/blob/master/sign.js

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import urllib.request

def decrypt_pwd(path: str, password: str) -> bytes:
    node = shutil.which("node")
    if node is None:
        raise Exception("Nodejs is not in path.")
    return subprocess.check_output(["node", "sign.js", path, password])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
                    prog='signing-util-creater',
                    description='Create the script for use in CI to sign the hap according to a build_profile.json5')
    parser.add_argument("build_profile_json")
    args = parser.parse_args()

    profile = json.load(open(args.build_profile_json))
    config = profile["app"]["signingConfigs"][0]["material"]

    basePath = Path(config["certpath"]).parent
    build_profile_path, filename = os.path.split(args.build_profile_json)

    keyPwd = decrypt_pwd(build_profile_path, config["keyPassword"]).strip()

    keystorePwd = decrypt_pwd(build_profile_path, config["storePassword"]).strip()
    key_base_name = config["certpath"].split("\\")[-1].split(".")[0]

    with open("sign.sh", "w") as f:
        f.write("#!/usr/bin/env bash\n")
        f.write("set -eu\n")
        f.write(f"key_base_name=\"{key_base_name}\"\n")
        f.write("java -jar /data/commandline-tools/sdk/default/openharmony/toolchains/lib/hap-sign-tool.jar ")
        f.write("sign-app ")
        f.write("-mode localSign ")
        f.write("-keyAlias debugKey ")
        f.write("-keystoreFile ${HOME}/.ohos/config/${key_base_name}.p12 ")
        f.write(f"-keystorePwd {keystorePwd.decode('utf-8')} ")
        f.write(f"-keyPwd {keyPwd.decode('utf-8')} ")
        f.write("-signAlg SHA256withECDSA ")
        f.write("-profileFile ${HOME}/.ohos/config/${key_base_name}.p7b ")
        f.write("-appCertFile ${HOME}/.ohos/config/${key_base_name}.cer ")
        f.write("-inFile $1 ")
        f.write("-outFile $2 ")
        print("File written")


