import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent / 'support'))
from gnome import Desktop

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def desktop():
    if os.environ.get('SHIKOMI_ISOLATED_TEST') != '1':
        pytest.skip('scripts/test-desktop.sh の隔離GNOMEで実行する')
    return Desktop()


@pytest.fixture
def target(tmp_path, desktop):
    observation = tmp_path / 'pasted.txt'
    environment = dict(os.environ, SHIKOMI_OBSERVATION=str(observation), GDK_BACKEND='wayland')
    process = subprocess.Popen([sys.executable, str(ROOT / 'tests/support/target.py')], env=environment)
    for _ in range(100):
        if observation.exists():
            break
        time.sleep(.05)
    assert observation.exists(), '入力先の起動に失敗'
    time.sleep(.3)
    desktop.focus_target()
    yield observation
    process.terminate()
    process.wait(timeout=5)
