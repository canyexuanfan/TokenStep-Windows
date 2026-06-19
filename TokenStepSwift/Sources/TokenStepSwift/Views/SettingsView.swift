import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var appState: AppState

    var body: some View {
        TabView {
            Form {
                Section("每日目标") {
                    Stepper(
                        value: Binding(
                            get: { appState.settings.dailyGoalTokens / 10_000_000 },
                            set: { appState.setGoal(max(1, $0) * 10_000_000) }
                        ),
                        in: 1...100
                    ) {
                        Text(TokenStepFormat.tokens(appState.settings.dailyGoalTokens))
                    }
                    Text("默认每天一个亿，可以按自己的节奏增减。")
                        .foregroundStyle(.secondary)
                }

                Section("自动刷新") {
                    Picker("刷新频率", selection: Binding(
                        get: { appState.settings.refreshIntervalSeconds },
                        set: { appState.setRefreshInterval($0) }
                    )) {
                        Text("1 分钟").tag(60)
                        Text("5 分钟").tag(300)
                        Text("15 分钟").tag(900)
                        Text("手动").tag(0)
                    }
                    .pickerStyle(.segmented)
                }

                Section("启动") {
                    Toggle("登录后自动启动 TokenStep", isOn: Binding(
                        get: { appState.autostartEnabled },
                        set: { appState.setAutostart($0) }
                    ))
                    Text("首次运行会默认开启；之后完全跟随这个开关，避免漏记每日 AI 步数。")
                        .foregroundStyle(.secondary)
                }
            }
            .tabItem { Label("通用", systemImage: "gearshape") }

            Form {
                Section("数据范围") {
                    Text("本地读取 Codex、Claude Code、Gemini 的 token 数量。")
                    Text("不读取代码，不上传对话。")
                        .foregroundStyle(.secondary)
                }
                Section("文件") {
                    Text(AppPaths.usageJSON.path)
                    Text(AppPaths.settingsJSON.path)
                }
            }
            .tabItem { Label("隐私", systemImage: "lock.shield") }
        }
        .frame(width: 520, height: 360)
        .padding()
    }
}
