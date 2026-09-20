"""実際のFlutter画面とGNOMEを通した結果を、受入例の各行へ対応させる。"""
import json
import os
from pathlib import Path
import subprocess

import pytest

ROOT = Path(__file__).resolve().parents[2]
CASES = json.loads((Path(__file__).with_name('gui-cases.json')).read_text())


@pytest.fixture(scope='module')
def gui_results():
    if os.environ.get('SHIKOMI_ISOLATED_TEST') != '1':
        pytest.skip('scripts/test-desktop.sh の隔離GNOMEで実行する')
    result = subprocess.run([
        str(ROOT / 'scripts/flutter.sh'), 'test', 'integration_test/gui_test.dart',
        '-d', 'linux', '--machine', f'--dart-define=SHIKOMI_ROOT={ROOT}',
    ], cwd=ROOT / 'client/shikomi/gui', capture_output=True, text=True, timeout=600)
    log = Path(os.environ['SHIKOMI_SESSION']) / 'gui-test.log'
    log.write_text(result.stdout + '\n' + result.stderr)
    names = {}
    results = {}
    for line in result.stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict):
            continue
        if event.get('type') == 'testStart':
            names[event['test']['id']] = event['test']['name']
        elif event.get('type') == 'testDone':
            results[names.get(event['testID'])] = event['result']
    return results, result, log


class TestGui:
    @pytest.mark.parametrize('case', CASES, ids=lambda case: case['id'])
    def test_desktop_journey(self, gui_results, case):
        results, process, log = gui_results
        assert results.get(case['id']) == 'success', f'{log}\n{process.stdout}\n{process.stderr}'
        assert process.returncode == 0, f'実画面テストの終了に失敗しました: {log}'
