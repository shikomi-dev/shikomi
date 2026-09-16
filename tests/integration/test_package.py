from pathlib import Path
import subprocess
import os

ROOT = Path(__file__).resolve().parents[2]


class TestPackage:
    def test_release_tag_must_match_version(self, tmp_path):
        result = subprocess.run([str(ROOT / 'scripts/build-deb.sh'), 'v999.0.0', str(tmp_path)], capture_output=True, text=True)
        assert result.returncode == 1
        assert '一致していません' in result.stderr
        assert not list(tmp_path.glob('*.deb'))

    def test_package_contains_public_commands_without_test_observer(self, tmp_path):
        version = (ROOT / 'VERSION').read_text().strip()
        subprocess.run([str(ROOT / 'scripts/build-deb.sh'), f'v{version}', str(tmp_path)], check=True, capture_output=True)
        package = tmp_path / f'shikomi_{version}_all.deb'
        assert subprocess.check_output(['dpkg-deb', '-f', str(package), 'Version'], text=True).strip() == version
        subprocess.run(['dpkg-deb', '-x', str(package), str(tmp_path / 'installed')], check=True)
        prefix = tmp_path / 'installed/usr'
        assert (prefix / 'bin/shikomi').is_file()
        assert (prefix / 'share/gnome-shell/extensions/shikomi@shikomi-dev.github.io/extension.js').is_file()
        assert not list(prefix.rglob('*observer*'))
        command = subprocess.run([str(prefix / 'bin/shikomi'), '--help'], capture_output=True, text=True)
        assert command.returncode == 0
        assert '{add,edit,remove,list}' in command.stdout

    def test_apt_index_has_a_verifiable_signature(self, tmp_path):
        version = (ROOT / 'VERSION').read_text().strip()
        signing = tmp_path / 'gnupg'
        signing.mkdir(mode=0o700)
        environment = dict(os.environ, GNUPGHOME=str(signing))
        try:
            subprocess.run(['gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase', '',
                            '--quick-generate-key', 'shikomi test', 'ed25519', 'sign', '0'],
                           env=environment, check=True, capture_output=True)
            public = tmp_path / 'public.gpg'
            public.write_bytes(subprocess.check_output(['gpg', '--batch', '--export'], env=environment))
            subprocess.run([str(ROOT / 'scripts/build-deb.sh'), f'v{version}', str(tmp_path)],
                           check=True, capture_output=True)
            output = tmp_path / 'apt'
            subprocess.run([str(ROOT / 'scripts/build-apt-repository.sh'),
                            str(tmp_path / f'shikomi_{version}_all.deb'), str(public), str(output)],
                           env=environment, check=True, capture_output=True)
            subprocess.run(['gpgv', '--keyring', str(public), str(output / 'dists/stable/InRelease')],
                           check=True, capture_output=True)
            index = (output / 'dists/stable/main/binary-amd64/Packages').read_text()
            assert 'Package: shikomi' in index
            assert f'Version: {version}' in index
        finally:
            subprocess.run(['gpgconf', '--kill', 'gpg-agent'], env=environment, check=True)
