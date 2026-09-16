# 開発への参加

変更を始める前に、[開発の進め方](docs/000_process.md)と[作業規則](AGENTS.md)を読んでください。

不具合や変更はIssueから始めます。実装内容ではなく、困っている場面、期待する体験、実際の入口と結果、完了条件を書きます。Issue番号が決まったら[増分の雛形](docs/210_increments/_template.md)を `INC-<Issue番号>.md` として使い、戻しにくく長く効く判断だけを[決定記録](docs/decisions/_template.md)へ分けます。

体験や全体構造を変える場合は、[要求の正典](docs/110_requirements/README.md)と[システムの現在像](docs/150_system/README.md)も同じPull Requestで更新します。

変更は専用ブランチで行い、テストと同じコミットに含めます。Pull Requestを作る前に次を実行してください。

```console
just check
```

Pull Requestには、中心の利用の流れを実際に通した入力と結果を書きます。未確認のことは、そのまま未確認と記載してください。
