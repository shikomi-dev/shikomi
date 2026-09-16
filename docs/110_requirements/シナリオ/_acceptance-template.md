# 目的名の確認

同じディレクトリのシナリオが保証することを、ケースから実行物へ結びます。入力と期待結果はテストデータまたは手動確認書を正本にし、ここへ写しません。

| ケース | 確かめること | 実行先 |
|---|---|---|
| EX-SC番号-01 | どの保証を確かめるか | `tests/acceptance/sc_番号/test_example.py::TestExample::test_example[EX-SC番号-01]` |

手動なら実行先を `tests/acceptance/manual/SC-番号.md::EX-SC番号-01` とし、確認書に同名の見出しを作ります。雛形はリポジトリ直下の `tests/acceptance/manual/_template.md` にあります。ケースの個数に合わせて行を増減します。
