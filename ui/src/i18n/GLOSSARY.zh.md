# 术语表（简体中文）

PolarExplorer 界面和帮助参考中文版所用的固定译法，与 `GLOSSARY.md` 的英、法、德术语表一一对应，使各区域及以后的功能说法一致。读者是领航员、航线规划人员和性能分析人员：优先使用中国帆船界、竞赛规则和航线规划软件的说法，而不是词典释义。

**审校状态：机器起草，需要以中文为母语的帆船人士审校。** 标有 **⚑** 的条目是最没有把握的，备注中说明了原因。

**语体。** 采用中国大陆 macOS / Windows 软件的写法；称呼读者为“你”。按钮和菜单用动词短语（“导出”“保存”），或名词（“设置”）。

**标点。** 中文行文使用全角标点（，。：；（）“”）。中文与拉丁字母、数字之间加一个空格（“导出 Expedition 文件”“0.05 kn”）；紧邻永不翻译的词、单位和括号中的纯 ASCII 内容时保留半角括号，例如“请求超时 (s)”。界面上的控件名在说明文字中用“”括起来。保留英文中的 `…`（表示会打开对话框）和每个 `{placeholder}`；快捷键和符号（⌘、▸、✕、°）原样保留。

**永不翻译：** PolarExplorer、ORC、ORR、TWA、TWS、BSP、VMG、COG、SOG、TWD、AWA、Expedition、Adrena、YellowBrick、Geovoile、Blue Water Tracks、GeoJSON、CSV、GRIB、ERA5、WebGL2、MMSI、文件扩展名（.wpsproj）、单位符号（kn、m/s、km/h、m、ft、nm、km、GB、MB、s、°）。速度单位符号在所有语言中都写 kn；行文中说“节”。

## 应用概念

| English | 中文 | 备注 |
|---|---|---|
| polar | 极曲线 | 数据本身。全书统一用“极曲线”，不用“极坐标图”。 |
| polar plot / polar diagram | 极曲线图 | 2D 图。 |
| blend (noun) | 融合极曲线；在设置名中简称“融合” | ⚑ 直白说法，并非航线软件术语；审校者或许更喜欢“合成极曲线”。 |
| blend (verb) | 融合 | |
| source | 来源 | 参与构成极曲线的任何东西。海流的“数据源”另用“数据源”。 |
| weight | 权重 | |
| output grid | 输出网格 | |
| coverage: direct / filled | 覆盖：直接 / 填充 | |
| confidence (full) | （完全）置信 | |
| overlay | 叠加层 | 存储在来源旁边的用户更改。 |
| track | 航迹 | |
| tracker | 追踪器 | 定位报告设备及其服务。 |
| race / event / leg / fleet | 比赛 / 赛事 / 赛段 / 船队 | |
| race key | 比赛代码 | YellowBrick 的比赛简称。 |
| sample (a track position) | 样本点；作为图上的点时称“点” | |
| polar segment | 极曲线片段 | |
| polar node | 极曲线节点 | 描述中也说“网格点”。 |
| exclude / include | 排除 / 纳入 | |
| leave out (filter) | 剔除 | “筛除”用于“已被筛选去掉”的状态。 |
| filter | 筛选 | |
| surface (3D) | 曲面 | |
| polar tower | 极曲线塔 | |
| Cartesian | 直角坐标 | |
| ORC certificate | ORC 证书 | |
| sister ship | 姊妹船 | |
| stage (Map / 3D / 2D / Compare) | 视图（地图 / 3D / 2D / 比较） | |
| Compare (noun and verb) | 比较 | `Compare@@verb` 同样译为“比较”。 |
| operand | 操作数 | 比较中的 A 和 B。 |
| heat map | 热图 | |
| cell | 单元格 | |
| grid | 网格 | |
| slice | 切片 | |
| dot band | 点带宽 | ⚑ |
| statistic | 统计量 | 第 90 百分位数、中位数、平均值。 |
| derive / derived | 推算 / 推算的 | |
| given / provided | 提供的 | |
| project | 项目 | |
| start screen | 起始屏幕 | |
| settings | 设置 | |
| theme | 主题 | Harbour 港湾、Midnight 午夜、Ocean 海洋、Plum 梅紫、Ember 余烬、Paper 纸张。 |
| autosave | 自动保存 | |
| recovered work | 恢复的工作 | |
| Save / Don't save / Cancel | 保存 / 不保存 / 取消 | 用“保存”（Windows 习惯），不用 macOS 的“存储”。 |
| Open Recent | 打开最近使用 | |
| copy (clipboard) | 复制 | |
| dialog | 对话框 | |
| tick / untick | 勾选 / 取消勾选 | |
| click | 点按 | macOS 中文写法。 |
| drag | 拖移 | |
| frame (fit on the map) | 缩放以显示 | |
| feature search | 功能搜索 | |
| control | 控件 | |
| equirectangular / orthographic | 等距圆柱投影 / 正射投影 | |
| catalogue | 目录 | |
| scrape | 抓取 | |
| token / rotate token | 令牌 / 更换令牌 | MCP 服务。 |

## 航海、气象和 ORC 术语

| English | 中文 | 备注 |
|---|---|---|
| TWA — true wind angle | 真风角 | 缩写保留 TWA。 |
| TWS — true wind speed | 真风速 | |
| TWD — true wind direction | 真风向 | |
| AWA — apparent wind angle | 视风角 | |
| BSP — boat speed | 船速 | |
| heading | 航向；“Boat heading”分组为“船首航向” | ⚑ |
| COG / SOG | 对地航向 / 对地航速 | 缩写保留。 |
| over the ground / through the water | 对地 / 对水 | |
| tack (manoeuvre) / gybe | 迎风换舷 / 顺风换舷 | |
| upwind / downwind | 迎风 / 顺风 | |
| to windward | 向上风 | VMG 说明中。 |
| beat angle / run angle (ORC) | 最佳迎风角 / 最佳顺风角 | |
| manoeuvre threshold | 操纵阈值 | ⚑ 也可作“机动阈值”。 |
| motoring | 机动航行 | ⚑ 指开机器航行。 |
| leeway | 风压差 | |
| wind / waves / current | 风 / 浪 / 海流 | 风和浪用“来向”，海流用“流向”（“{speed}，流向 {direction}”）。 |
| wave height / significant | 浪高 / 有效波高 | |
| wave period | 浪周期 | |
| wave direction / mean wave direction | 浪向 / 平均浪向 | |
| head / bow / beam / quarter / following seas | 迎浪 / 首斜浪 / 横浪 / 尾斜浪 / 顺浪 | 浪向扇区。 |
| angle off the bow | 相对船首的角度 | |
| swell / sea state | 涌浪 / 海况 | |
| tide, tidal current | 潮汐，潮流 | “不含潮汐的海流”。 |
| Stokes drift | 斯托克斯漂移 | |
| correct for current | 修正海流影响 | |
| reanalysis | 再分析 | |
| weather (fetched wind, waves, current) | 气象数据 | “获取气象数据…”。 |
| environment | 环境数据 | |
| fetch | 获取 | 一般的下载说“下载”。 |
| hourly / every 3 hours | 逐小时 / 每 3 小时 | |
| compass direction | 罗经方位 | |
| sail number | 帆号 | |
| builder / designer / year built | 船厂 / 设计师 / 建造年份 | |
| class / division / handicap class | 级别 / 组别 / 让分级别 | ⚑ |
| Measurements (ORC) | 丈量数据 | 总长、船宽、吃水、排水量、主帆、热那亚帆、球帆、不对称球帆面积。 |
| start / finish | 起航 / 终点（完赛） | |
| Racing / Finished / Retired / Did not start / Did not finish | 比赛中 / 已完赛 / 退赛 / 未起航 / 未完赛 | ⚑ 请对照中国帆船竞赛规则中 RET、DNS、DNF 的译法。 |
| knots / nautical miles | 节 / 海里 | 符号 kn、nm。 |
| local solar time | 地方太阳时 | 一天中的时段：夜间、上午、下午、傍晚。 |
| Hindsight Sync | Hindsight Sync | 产品名，不翻译。 |
| reanalysis archive | 再分析存档 | |
| Scan (button) | 扫描 | |
