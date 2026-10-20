# 用語集（日本語）

The Japanese words the interface and the help reference use for
PolarExplorer's concepts, fixed so every area says the same thing. Same
conventions as `GLOSSARY.md`. **Review status of every entry: machine-drafted,
needs a native-speaking sailor.** Entries marked **⚑** are the least certain.

**Register.** Sentences, tooltips and help text are polite です／ます. Buttons
and menu items are short nouns or verbs as macOS Japanese writes them
（"保存", "開く…", "書き出す…", "取り消す", "やり直す"）. The native menu in
`crates/pe-app/src/menu.rs` already uses these words.

**Punctuation and spacing.** Full-width 、。：（）「」 in running text. A
half-width space between Japanese and Latin words or numbers, as Apple does
（"Expedition ファイル", "60 kn", "{count} 件"）. Keep `…` where the English has
it (it opens a dialog), every `{placeholder}`, and ⌘, ▸, ✕, °.

**Never translated:** as in `GLOSSARY.md` — PolarExplorer, ORC, ORR, TWA, TWS,
BSP, VMG, COG, SOG, TWD, AWA, Expedition, Adrena, YellowBrick, Geovoile, Blue
Water Tracks, GeoJSON, CSV, GRIB, ERA5, file extensions, unit symbols (kn,
m/s, km/h, m, ft, nm, km, GB, s, °), key names (Shift, Esc, Enter, Tab).

## Application terms

| English | 日本語 | Note |
|---|---|---|
| polar | ポーラー | The data. Never "極座標". |
| polar plot | ポーラー図 | The 2D drawing. |
| blend (noun / verb) | ブレンド／ブレンドする | ⚑ A reviewer may prefer 合成ポーラー／合成する. |
| Blend settings | ブレンド設定 | |
| source | ソース | Anything that contributes to the polar. |
| weight | 重み | |
| output grid | 出力グリッド | |
| coverage (direct / filled) | カバー状況（直接／補間） | |
| confidence (full) | 信頼度（最大） | |
| export | 書き出す／書き出し | macOS wording. Import: 読み込む／読み込み. |
| overlay | オーバーレイ | A user change stored beside a source. |
| track | 航跡 | |
| tracker | トラッカー | |
| sample (a track position) | サンプル点 | 「点」 alone where space is short. |
| polar segment | ポーラーセグメント | |
| polar node | ポーラーの格子点 | |
| exclude / include | 除外する／含める | |
| surface (3D) | 曲面 | |
| polar tower | ポーラータワー | |
| ORC certificate | ORC 証書 | ⚑ レーティング証書 in prose where ORC is not named. |
| certificate year | 証書年度 | |
| catalogue | カタログ | |
| sister ship | 姉妹艇 | |
| project | プロジェクト | |
| start screen | スタート画面 | |
| stage | ビュー | Map / 3D / 2D / Compare: 地図／3D／2D／比較. |
| navigation (left panel) | ナビゲーション | |
| settings | 設定 | |
| theme | テーマ | Harbour: ハーバー; Midnight: ミッドナイト. |
| autosave | 自動保存 | |
| recovered work | 復元された作業 | |
| unsaved changes | 未保存の変更 | |
| Save / Don't save / Cancel | 保存／保存しない／キャンセル | |
| Save As… | 別名で保存… | |
| Open Recent | 最近使った項目を開く | |
| undo / redo | 取り消す／やり直す | "One undo" → 1 回の取り消し. |
| feature search | 機能検索 | |
| control (UI) | コントロール | |
| dialog | ダイアログ | |
| tick / untick | チェックを入れる／外す | |
| leave out (filter) | 除く | |
| frame (fit on the map) | 全体を表示 | |
| derive / derived | 算出する／算出値 | Heading and speed from positions. |
| given (value from the file) | ファイルの値 | |
| add | 追加 | |
| boat | 艇 | |
| colour | 色 | |
| equirectangular / orthographic | 正距円筒図法／正射図法 | |
| filter | フィルタ | |
| smooth | 平滑化 | |
| time of day / daytime | 時間帯／日中 | |
| measure (tool) | 計測 | |

## Sailing, meteorological and ORC terms

| English | 日本語 | Note |
|---|---|---|
| TWA — true wind angle | 真風向角 | Keep "TWA" as the abbreviation. |
| TWS — true wind speed | 真風速 | Keep "TWS". |
| BSP — boat speed | 艇速 | Keep "BSP". |
| heading | 船首方位 | ⚑ Some sailors say ヘディング. |
| tack (side) | タック（スターボードタック／ポートタック） | |
| to tack / to gybe | タックする／ジャイブする | Nouns タック／ジャイブ. |
| manoeuvre | マニューバー | |
| bear away / luff up | ベアウェイ／ラフアップ | |
| upwind / downwind | アップウィンド／ダウンウィンド | クローズホールド, ランニング for the points of sail. |
| beat angle / run angle (ORC) | 最適アップウィンド角／最適ダウンウィンド角 | |
| closer to / further off the wind | より風上へ／より風下へ | |
| wind / waves / current | 風／波／海流 | Current is "toward", wind and waves "from". ⚑ 潮流 when tide is meant. |
| tide, tidal current | 潮汐、潮流 | "currents without tide": 潮汐を除いた海流. |
| current set / drift | 流向／流速 | |
| wave height (significant) | 波高（有義波高） | |
| mean wave direction | 平均波向 | |
| head / bow / beam / quarter / following seas | 向かい波／斜め前からの波／横波／斜め後ろからの波／追い波 | |
| sea state | 海況 | |
| Stokes drift | ストークスドリフト | |
| correct for current | 海流を補正する | |
| over the ground / through the water | 対地／対水 | |
| heel / leeway | ヒール／リーウェイ | |
| reanalysis | 再解析 | |
| weather (fetched wind, waves, current) | 気象データ | "Fetch weather…": 気象データを取得…. |
| environment (of a sample) | 環境データ | |
| fetch | 取得 | Downloads in general: ダウンロード. |
| hourly / every 3 hours | 1 時間ごと／3 時間ごと | |
| statistic | 統計量 | 90th percentile: 90 パーセンタイル; median: 中央値; mean: 平均. |
| cell / grid / slice | セル／グリッド／断面 | |
| dot band (TWS tolerance) | 風速幅 | |
| heat map | ヒートマップ | |
| knots / nautical miles | ノット／海里 | Symbols kn, nm. |
| sail number | セール番号 | |
| builder / designer / year built | ビルダー／デザイナー／建造年 | |
| class, division | クラス、ディビジョン | |
| handicap class | ハンディキャップクラス | |
| race / leg / fleet / event | レース／レグ／フリート／イベント | |
| race key | レースキー | YellowBrick's short race name. |
| start / finish | スタート／フィニッシュ | |
| Racing / Finished / Retired / Did not start / Did not finish | レース中／フィニッシュ／リタイア／未スタート（DNS）／未フィニッシュ（DNF） | ⚑ |
| motoring | 機走 | |
| Hindsight Sync | Hindsight Sync | 製品名。翻訳しない。 |
| reanalysis archive | 再解析アーカイブ | |
| Scan (button) | スキャン | |
