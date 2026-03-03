LICENSE for this code is MIT according to [https://github.com/TermonyHQ/Termony]().

To use the script:
- Clone [https://github.com/jschwe/ServoDemo/]() and open in DevEco Studio.
- Generate valid signing config via 'Project Structure' -> 'Signing Configs'
- Check 'Automatically generate signature' and login with your developer account.
- This will create a build-profile.json5 with the valid key material.
- Copy the 'build-profile.json5' and files in '~/.ohos/config' into '/tmp/certpath'.
- Run: `uv run main.py /tmp/certpath/build-profile.json5` or wherever the key material, certificate and build-profile.json5 are.