# Pahcer Web Implementation Tasks

## Phase 1: 初期セットアップとアーキテクチャ設計
- [x] **プロジェクト構造確認 & 依存関係整理**
    - [x] `pahcer-web-backend`: Hono, zod 等のセットアップ (JSON操作用ライブラリ含む)
    - [x] `pahcer-web-frontend`: Svelte 5, Vite, TailwindCSS (必要であれば) 等のセットアップ
- [x] **データ永続化層の設計 (JSONベース)**
    - [x] JSONデータスキーマ設計 (実行ジョブメタデータ, 設定等)
    - [x] `XDG_CONFIG_HOME` 準拠のグローバル設定保存ロジック実装
    - [x] `.pahcer-web` (各プロジェクト用) ディレクトリへのローカル設定保存ロジック実装

## Phase 2: バックエンド実装 (`pahcer-web-backend`)
- [ ] **CLI エントリーポイント実装**
    - [x] `pahcer-web ui` コマンドの実装 (Honoサーバー起動)
    - [ ] `pahcer` コマンドのラッパー機能 (init/run/list/prune/help) の基本実装
- [x] **テスト実行機能 (`pahcer run`)**
    - [x] `child_process` による `pahcer run` 実行ロジック
    - [x] SSE (Server-Sent Events) による標準出力・標準エラー出力のストリーミング配信 API
    - [x] **実行結果の詳細保存**
        - [x] 実行完了後に `pahcer` の出力結果（スコア、ステータス等）を解析し、構造化データとして保存する機能実装
- [x] **評価履歴機能 (`pahcer list`)**
    - [x] `pahcer list` の結果解析またはログファイル読み込み API
    - [x] ビジュアライザ (HTML) のダウンロード機能
    - [x] ビジュアライザ用静的ファイル配信機能
- [x] **スコア分析機能**
    - [x] `https://img.atcoder.jp/ahc_standings/index.html` ダウンロード・保存機能
    - [x] `input.csv`, `result.csv` 生成ロジック
    - [x] 分析用データ提供 API
- [x] **設定管理機能**
    - [x] 設定値 (ビジュアライザURL, 表示位置, seed/scale初期値) の保存・読み出し API (JSON)

## Phase 3: フロントエンド実装 (`pahcer-web-frontend`)
- [x] **基本UI構成 (SPA)**
    - [x] タブナビゲーション実装 (テスト実行, 評価履歴, スコア分析, 設定)
    - [x] Svelte 5 Runes を用いたグローバル状態管理設計
- [x] **テスト実行タブ**
    - [x] 実行オプション入力フォーム
    - [x] 実行ボタン & 中断ボタン
    - [x] リアルタイムログ表示コンソール (SSE受信)
- [x] **評価履歴タブ**
    - [x] 結果一覧テーブル表示
    - [x] ビジュアライザ表示用 `iframe` 実装
    - [x] seed/scale 変更時の `iframe` 内更新ロジック
- [x] **スコア分析タブ**
    - [x] スコア比較・分析ビューの実装
- [x] **設定タブ**
    - [x] 設定変更フォーム実装

## Phase 4: 統合と仕上げ
- [ ] **結合**
    - [x] フロントエンドのビルド成果物をバックエンドから配信する設定
    - [x] `pahcer-web` コマンド一つでアプリが起動することの確認
- [ ] **動作検証**
    - [ ] `pahcer run` のストリーミング動作確認
    - [ ] ビジュアライザのダウンロードと表示確認
    - [ ] 各種設定の保存・反映確認 (グローバル/ローカル)
- [ ] **npm パッケージ化準備**
    - [ ] `package.json` の整理 (bin 定義等)
    - [ ] README ドキュメント整備
