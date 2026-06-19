import AppKit
import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var appState: AppState

    private let columns = [
        GridItem(.flexible(), spacing: 18),
        GridItem(.flexible(), spacing: 18)
    ]

    var body: some View {
        ZStack {
            TokenStepBackdrop()

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 22) {
                    header

                    LazyVGrid(columns: columns, spacing: 18) {
                        dailyGoalCard
                        themeCard
                        refreshCard
                        updateCard
                        autostartCard
                        privacyCard
                    }

                    footer
                }
                .padding(.top, 36)
                .padding(.horizontal, 34)
                .padding(.bottom, 24)
            }
        }
        .frame(width: 920, height: 760)
        .id(appState.settings.theme.id)
    }

    private var header: some View {
        HStack(spacing: 14) {
            TokenStepMark(size: 54)

            VStack(alignment: .leading, spacing: 5) {
                Text("设置")
                    .font(.system(size: 34, weight: .heavy, design: .rounded))
                    .foregroundStyle(Color.tokenInk)
                Text("让 TokenStep 按你的节奏记录 Token 消耗")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(.secondary)
            }

            Spacer()

            HStack(spacing: 7) {
                Circle()
                    .fill(Color.tokenGreen)
                    .frame(width: 8, height: 8)
                Text("本地统计")
                    .font(.callout.weight(.heavy))
                    .foregroundStyle(Color.tokenGreenDark)
            }
            .padding(.horizontal, 15)
            .padding(.vertical, 10)
            .background(Color.tokenMint.opacity(0.22), in: Capsule())
            .overlay(Capsule().stroke(Color.tokenGreen.opacity(0.12)))
        }
    }

    private var dailyGoalCard: some View {
        SettingsCard(title: "每日目标", symbol: "figure.walk.circle.fill") {
            VStack(alignment: .leading, spacing: 18) {
                HStack(alignment: .center, spacing: 18) {
                    ZStack {
                        ProgressRingView(progress: min(appState.progress, 1), lineWidth: 8)
                            .frame(width: 82, height: 82)
                        Text(TokenStepFormat.percent(min(appState.progress * 100, 999)))
                            .font(.system(size: 18, weight: .heavy, design: .rounded))
                            .foregroundStyle(Color.tokenInk)
                            .monospacedDigit()
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        Text(TokenStepFormat.tokens(appState.settings.dailyGoalTokens))
                            .font(.system(size: 34, weight: .heavy, design: .rounded))
                            .foregroundStyle(Color.tokenInk)
                            .monospacedDigit()
                        Text("token / 天")
                            .font(.callout.weight(.bold))
                            .foregroundStyle(.secondary)
                    }

                    Spacer()
                }

                HStack(spacing: 10) {
                    GoalStepButton(symbol: "minus") {
                        appState.setGoal(appState.settings.dailyGoalTokens - 10_000_000)
                    }
                    .disabled(appState.settings.dailyGoalTokens <= 10_000_000)

                    GoalStepButton(symbol: "plus") {
                        appState.setGoal(appState.settings.dailyGoalTokens + 10_000_000)
                    }

                    Spacer()
                }

                HStack(spacing: 8) {
                    ForEach([50_000_000, 100_000_000, 200_000_000], id: \.self) { value in
                        PresetChip(
                            title: TokenStepFormat.tokens(value),
                            selected: appState.settings.dailyGoalTokens == value
                        ) {
                            appState.setGoal(value)
                        }
                    }
                }
            }
        }
    }

    private var refreshCard: some View {
        SettingsCard(title: "自动刷新", symbol: "arrow.triangle.2.circlepath.circle.fill") {
            VStack(alignment: .leading, spacing: 18) {
                Text("菜单栏、弹层和仪表盘会按这个频率同步更新。")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(.secondary)

                HStack(spacing: 8) {
                    ForEach(refreshOptions) { option in
                        RefreshOptionButton(
                            title: option.title,
                            selected: appState.settings.refreshIntervalSeconds == option.seconds
                        ) {
                            appState.setRefreshInterval(option.seconds)
                        }
                    }
                }

                StatusLine(
                    symbol: appState.settings.refreshIntervalSeconds == 0 ? "hand.raised.fill" : "timer",
                    title: "当前节奏",
                    value: appState.settings.refreshIntervalSeconds == 0 ? "手动更新" : "每 \(TokenStepFormat.intervalLabel(appState.settings.refreshIntervalSeconds))",
                    tint: .tokenGreen
                )

                Spacer(minLength: 0)
            }
        }
    }

    private var themeCard: some View {
        SettingsCard(title: "主题色", symbol: "paintpalette.fill") {
            VStack(alignment: .leading, spacing: 16) {
                Text("菜单栏、圆环、活动墙和按钮会一起跟随主题变化。")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                HStack(spacing: 9) {
                    ForEach(TokenStepTheme.allCases) { theme in
                        ThemeSwatchButton(
                            theme: theme,
                            selected: appState.settings.theme == theme
                        ) {
                            appState.setTheme(theme)
                        }
                    }
                }

                StatusLine(
                    symbol: "sparkles",
                    title: "当前主题",
                    value: appState.settings.theme.title,
                    tint: .tokenGreen
                )

                Spacer(minLength: 0)
            }
        }
    }

    private var autostartCard: some View {
        SettingsCard(title: "开机启动", symbol: "power.circle.fill") {
            VStack(alignment: .leading, spacing: 18) {
                HStack(alignment: .top, spacing: 14) {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("登录后自动启动 TokenStep")
                            .font(.headline.weight(.heavy))
                            .foregroundStyle(Color.tokenInk)
                        Text("像步数一样默默记录，避免漏掉每天的 Token 消耗。")
                            .font(.callout.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    Spacer()

                    Toggle("", isOn: Binding(
                        get: { appState.autostartEnabled },
                        set: { appState.setAutostart($0) }
                    ))
                    .labelsHidden()
                    .toggleStyle(.switch)
                }

                StatusLine(
                    symbol: appState.autostartEnabled ? "checkmark.circle.fill" : "pause.circle.fill",
                    title: appState.autostartEnabled ? "已开启" : "已关闭",
                    value: appState.autostartEnabled ? "下次登录会自动运行" : "需要手动启动 App",
                    tint: appState.autostartEnabled ? .tokenGreen : .gray
                )
            }
        }
    }

    private var updateCard: some View {
        SettingsCard(title: "自动更新", symbol: "arrow.down.circle.fill") {
            VStack(alignment: .leading, spacing: 14) {
                HStack(alignment: .top, spacing: 14) {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("自动检查 TokenStep 新版本")
                            .font(.headline.weight(.heavy))
                            .foregroundStyle(Color.tokenInk)
                        Text("有更新时先提醒你，下载前会确认。")
                            .font(.callout.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    Spacer()

                    Toggle("", isOn: Binding(
                        get: { appState.settings.autoUpdateEnabled },
                        set: { appState.setAutoUpdateEnabled($0) }
                    ))
                    .labelsHidden()
                    .toggleStyle(.switch)
                }

                VStack(spacing: 8) {
                    SettingsToggleRow(
                        title: "下载前询问",
                        isOn: Binding(
                            get: { appState.settings.askBeforeDownloadingUpdates },
                            set: { appState.setAskBeforeDownloadingUpdates($0) }
                        )
                    )
                    SettingsToggleRow(
                        title: "仅安装已签名公证版本",
                        isOn: Binding(
                            get: { appState.settings.requireVerifiedUpdates },
                            set: { appState.setRequireVerifiedUpdates($0) }
                        )
                    )
                }

                HStack(spacing: 10) {
                    StatusLine(
                        symbol: appState.availableUpdate == nil ? "checkmark.circle.fill" : "arrow.down.circle.fill",
                        title: appState.availableUpdate == nil ? "当前版本 \(UpdateService.currentVersion)" : "发现 \(appState.availableUpdate?.version ?? "")",
                        value: updateCheckStatus,
                        tint: appState.availableUpdate == nil ? .tokenGreen : .tokenGreenDark
                    )

                    Button {
                        appState.checkForUpdates(silent: false)
                    } label: {
                        Text(appState.isCheckingForUpdates ? "检查中" : "检查更新")
                            .font(.caption.weight(.heavy))
                            .frame(width: 76, height: 34)
                    }
                    .buttonStyle(SettingsSecondaryButtonStyle())
                    .disabled(appState.isCheckingForUpdates)
                }
            }
        }
    }

    private var privacyCard: some View {
        SettingsCard(title: "隐私状态", symbol: "checkmark.shield.fill") {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 12) {
                    Image(systemName: "lock.shield.fill")
                        .font(.system(size: 18, weight: .heavy))
                        .foregroundStyle(Color.tokenGreenDark)
                        .frame(width: 38, height: 38)
                        .background(Color.tokenMint.opacity(0.30), in: Circle())

                    VStack(alignment: .leading, spacing: 3) {
                        Text("本地隐私保护已开启")
                            .font(.headline.weight(.heavy))
                            .foregroundStyle(Color.tokenInk)
                        Text("代码与对话不会离开这台 Mac")
                            .font(.caption.weight(.heavy))
                            .foregroundStyle(Color.tokenGreenDark)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.tokenMint.opacity(0.18), in: RoundedRectangle(cornerRadius: 17, style: .continuous))

                VStack(spacing: 8) {
                    PrivacyCheckRow(title: "只统计 token 数量")
                    PrivacyCheckRow(title: "不读取代码与对话")
                    PrivacyCheckRow(title: "不默认上传数据")
                }

                HStack(spacing: 8) {
                    PrivacyMetaChip(title: "本机")
                    PrivacyMetaChip(title: TokenStepFormat.generatedTime(appState.snapshot.generatedAt))
                    PrivacyMetaChip(title: "\(appState.snapshot.sources.count) 个客户端")
                }
            }
        }
    }

    private var footer: some View {
        HStack {
            Text("TokenStep · Local usage tracker")
                .font(.caption.weight(.bold))
                .foregroundStyle(.secondary)

            Spacer()

            Button {
                appState.setGoal(TokenStepSettings.defaults.dailyGoalTokens)
                appState.setRefreshInterval(TokenStepSettings.defaults.refreshIntervalSeconds)
                appState.setTheme(TokenStepSettings.defaults.theme)
                appState.setAutoUpdateEnabled(TokenStepSettings.defaults.autoUpdateEnabled)
                appState.setAskBeforeDownloadingUpdates(TokenStepSettings.defaults.askBeforeDownloadingUpdates)
                appState.setRequireVerifiedUpdates(TokenStepSettings.defaults.requireVerifiedUpdates)
                appState.setAutostart(true)
            } label: {
                Text("恢复默认")
                    .font(.callout.weight(.bold))
                    .frame(width: 92, height: 36)
            }
            .buttonStyle(SettingsSecondaryButtonStyle())

            Button {
                SettingsWindowPresenter.shared.close()
                NSApp.keyWindow?.close()
            } label: {
                Text("完成")
                    .font(.callout.weight(.heavy))
                    .frame(width: 82, height: 36)
            }
            .buttonStyle(SettingsPrimaryButtonStyle())
        }
    }

    private var refreshOptions: [RefreshOption] {
        [
            RefreshOption(seconds: 60, title: "1 分钟"),
            RefreshOption(seconds: 300, title: "5 分钟"),
            RefreshOption(seconds: 900, title: "15 分钟"),
            RefreshOption(seconds: 0, title: "手动")
        ]
    }

    private var updateCheckStatus: String {
        if appState.isCheckingForUpdates {
            return "正在检查"
        }
        if appState.availableUpdate != nil {
            return "可更新"
        }
        guard let date = appState.lastUpdateCheckAt else {
            return "尚未检查"
        }
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return "\(formatter.string(from: date))"
    }
}

private struct RefreshOption: Identifiable {
    var id: Int { seconds }
    var seconds: Int
    var title: String
}

private struct SettingsCard<Content: View>: View {
    var title: String
    var symbol: String
    var content: Content

    init(title: String, symbol: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.symbol = symbol
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: symbol)
                    .font(.system(size: 17, weight: .bold))
                    .foregroundStyle(Color.tokenGreenDark)
                    .frame(width: 28, height: 28)
                    .background(Color.tokenMint.opacity(0.22), in: Circle())
                Text(title)
                    .font(.title3.weight(.heavy))
                    .foregroundStyle(Color.tokenInk)
                Spacer()
            }

            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .padding(22)
        .frame(height: 238)
        .background(Color.tokenSurface, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).stroke(Color.black.opacity(0.06)))
        .shadow(color: Color.black.opacity(0.055), radius: 22, x: 0, y: 14)
    }
}

private struct GoalStepButton: View {
    var symbol: String
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .heavy))
                .foregroundStyle(Color.tokenInk.opacity(0.78))
                .frame(width: 34, height: 30)
                .background(Color.tokenTrack.opacity(0.55), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.black.opacity(0.05)))
        }
        .buttonStyle(.plain)
    }
}

private struct ThemeSwatchButton: View {
    var theme: TokenStepTheme
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 7) {
                ZStack {
                    Circle()
                        .fill(
                            LinearGradient(
                                colors: [
                                    theme.palette.accentSoft.color,
                                    theme.palette.accent.color,
                                    theme.palette.accentDark.color
                                ],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                        .frame(width: 38, height: 38)
                        .shadow(color: theme.palette.accentDark.color.opacity(selected ? 0.22 : 0.10), radius: 8, x: 0, y: 4)

                    if selected {
                        Image(systemName: "checkmark")
                            .font(.system(size: 13, weight: .heavy))
                            .foregroundStyle(.white)
                    }
                }

                VStack(spacing: 1) {
                    Text(theme.title)
                        .font(.caption.weight(.heavy))
                        .foregroundStyle(selected ? Color.tokenInk : Color.tokenInk.opacity(0.74))
                    Text(theme.subtitle)
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity)
            .frame(height: 88)
            .background(selected ? theme.palette.accentSoft.color.opacity(0.22) : Color.tokenTrack.opacity(0.24), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(selected ? theme.palette.accent.color.opacity(0.46) : Color.black.opacity(0.045), lineWidth: selected ? 1.4 : 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .help("切换到\(theme.title)主题")
    }
}

private struct PresetChip: View {
    var title: String
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.caption.weight(.heavy))
                .foregroundStyle(selected ? Color.white : Color.tokenInk.opacity(0.72))
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(selected ? Color.tokenGreen : Color.tokenTrack.opacity(0.45), in: Capsule())
                .overlay(Capsule().stroke(selected ? Color.clear : Color.black.opacity(0.045)))
        }
        .buttonStyle(.plain)
    }
}

private struct RefreshOptionButton: View {
    var title: String
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if selected {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11, weight: .heavy))
                }
                Text(title)
                    .font(.callout.weight(.heavy))
            }
            .foregroundStyle(selected ? Color.white : Color.tokenInk.opacity(0.7))
            .frame(maxWidth: .infinity)
            .frame(height: 42)
            .background(selected ? Color.tokenGreen : Color.tokenTrack.opacity(0.42), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(selected ? Color.clear : Color.black.opacity(0.035)))
        }
        .buttonStyle(.plain)
    }
}

private struct SettingsToggleRow: View {
    var title: String
    @Binding var isOn: Bool

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: isOn ? "checkmark.circle.fill" : "circle")
                .font(.system(size: 13, weight: .heavy))
                .foregroundStyle(isOn ? Color.tokenGreen : Color.secondary.opacity(0.65))
            Text(title)
                .font(.callout.weight(.bold))
                .foregroundStyle(Color.tokenInk.opacity(0.78))
            Spacer()
            Toggle("", isOn: $isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
        }
    }
}

private struct StatusLine: View {
    var symbol: String
    var title: String
    var value: String
    var tint: Color

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .foregroundStyle(tint)
            Text(title)
                .font(.callout.weight(.heavy))
                .foregroundStyle(Color.tokenInk)
            Spacer()
            Text(value)
                .font(.caption.weight(.bold))
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 13)
        .padding(.vertical, 10)
        .background(Color.tokenTrack.opacity(0.3), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

private struct SettingsInfoRow: View {
    var label: String
    var value: String

    var body: some View {
        HStack {
            Text(label)
                .font(.caption.weight(.bold))
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.caption.weight(.heavy))
                .foregroundStyle(Color.tokenInk.opacity(0.78))
                .lineLimit(1)
        }
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(Color.tokenTrack.opacity(0.28), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

private struct PrivacyCheckRow: View {
    var title: String

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 13, weight: .heavy))
                .foregroundStyle(Color.tokenGreen)
            Text(title)
                .font(.callout.weight(.bold))
                .foregroundStyle(Color.tokenInk.opacity(0.78))
            Spacer(minLength: 0)
        }
    }
}

private struct PrivacyMetaChip: View {
    var title: String

    var body: some View {
        Text(title)
            .font(.caption.weight(.heavy))
            .foregroundStyle(Color.tokenInk.opacity(0.66))
            .lineLimit(1)
            .minimumScaleFactor(0.74)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .background(Color.tokenTrack.opacity(0.35), in: Capsule())
    }
}

struct SettingsPrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(.white)
            .background(Color.tokenGreen, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .opacity(configuration.isPressed ? 0.82 : 1)
    }
}

struct SettingsSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(Color.tokenInk.opacity(0.72))
            .background(Color.tokenTrack.opacity(0.62), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.black.opacity(0.045)))
            .opacity(configuration.isPressed ? 0.76 : 1)
    }
}
