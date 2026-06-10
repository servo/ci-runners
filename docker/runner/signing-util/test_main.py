import unittest

from main import key_base_name_from_certpath


class KeyBaseNameFromCertpathTest(unittest.TestCase):
    def test_linux_certpath(self):
        self.assertEqual(
            key_base_name_from_certpath(
                "/home/username1/.ohos/config/default_ServoDemo-main_x-oq6isVY4eIntKSPgZ5kmdxrvhjVXfNufwJlEK3qwU=.cer"
            ),
            "default_ServoDemo-main_x-oq6isVY4eIntKSPgZ5kmdxrvhjVXfNufwJlEK3qwU=",
        )

    def test_windows_certpath(self):
        self.assertEqual(
            key_base_name_from_certpath(
                r"C:\Users\username1\.ohos\config\default_ServoDemo-main_x-oq6isVY4eIntKSPgZ5kmdxrvhjVXfNufwJlEK3qwU=.cer"
            ),
            "default_ServoDemo-main_x-oq6isVY4eIntKSPgZ5kmdxrvhjVXfNufwJlEK3qwU=",
        )

    def test_certpath_with_dots_in_filename(self):
        self.assertEqual(
            key_base_name_from_certpath("/home/username1/.ohos/config/debug.signing.key.cer"),
            "debug.signing.key",
        )


if __name__ == "__main__":
    unittest.main()
