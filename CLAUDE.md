# ALICE-Cloud-Gateway — Claude Code 設定

## プロジェクト概要

Cloud gateway for ALICE edge-to-cloud SDF streaming pipeline

| 項目 | 値 |
|------|-----|
| クレート名 | `alice-cloud-gateway` |
| バージョン | 0.1.0 |
| ライセンス | AGPL-3.0 |
| リポジトリ | `ext-sakamoro/ALICE-Cloud-Gateway` |
| Eco-Systemブリッジ | bridge_cloud_gateway.rs |

## コーディングルール

メインCLAUDE.md「Git Commit設定」参照。日本語コミット・コメント、署名禁止、作成者 `Moroya Sakamoto`。

## ALICE 品質基準

ALICE-KARIKARI.md「100/100品質基準」参照。clippy基準: `pedantic+nursery`

| 指標 | 値 |
|------|-----|
| clippy (pedantic+nursery) | 0 warnings |
| テスト数 | 148 |
| fmt | clean |

## Eco-System パイプライン

本クレートはALICE-Eco-Systemの以下のパスで使用:
- クラウドインフラ（Edge→Cloud SDF配信）

## 情報更新ルール

- バージョンアップ時: このCLAUDE.mdのバージョンを更新
- APIの破壊的変更時: ALICE-Eco-Systemブリッジへの影響をメモ
- テスト数/品質の変化時: 品質基準セクションを更新
- 新feature追加時: プロジェクト概要テーブルを更新
