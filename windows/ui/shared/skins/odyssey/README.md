# Odyssey popover artwork

这三张 PNG 是 TokenStep 0.2.4 菜单栏浮层使用的无字背景板，均由本轮内置 `imagegen` 生成，并以对应概念稿作为构图和氛围参考。

- `OdysseyAegeanPopover.png`：爱琴海冷雾；导演剪辑的浮层也复用此图；
- `OdysseyTrojanPopover.png`：特洛伊火海；
- `OdysseyAshMarblePopover.png`：灰烬神像。

共同提示词约束：`16:10` 原生 macOS 浮层背景板；电影海报级光影；主题主体保留在左侧或中右侧；为真实 UI 数据保留暗部负空间；禁止 UI、图表、按钮、Logo、文字、数字、水印、演员或可识别人物。

三套主题分别要求：

- 冷雾：午夜蓝海、岩崖、青铜与蓝钢头盔、竖向青铜骨节冠；
- 火海：焦黑木马、火焰背光、烟尘和克制的余烬；
- 灰烬：破损大理石战士、玄武岩、裂纹、灰尘与低亮火星。

运行时先从 App Bundle 读取；静态渲染脚本通过 `TOKENSTEP_ODYSSEY_*_ART_PATH` 注入同一文件。背景板没有烘焙任何演示数据，所有数字和控件仍由 SwiftUI 实时绘制。
