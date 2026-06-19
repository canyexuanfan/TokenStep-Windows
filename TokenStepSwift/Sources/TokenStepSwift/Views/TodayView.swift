import SwiftUI

struct TodayView: View {
    @EnvironmentObject private var appState: AppState

    var body: some View {
        VStack(spacing: 22) {
            hero
            metricStrip
            TokenCard {
                VStack(alignment: .leading, spacing: 18) {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("最近 30 天")
                                .font(.title3.weight(.heavy))
                                .foregroundStyle(Color.tokenInk)
                            Text("细线是每日目标")
                                .font(.callout.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text("今天 \(TokenStepFormat.tokens(appState.today.totalTokens, compact: true))")
                            .font(.callout.weight(.bold))
                            .foregroundStyle(Color.tokenGreenDark)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 7)
                            .background(Color.tokenMint.opacity(0.28), in: Capsule())
                    }
                    ActivityBarsView(rows: appState.snapshot.daily, goal: appState.settings.dailyGoalTokens)
                        .frame(height: 96)
                }
            }

            HStack(alignment: .top, spacing: 22) {
                distributionCard(title: "按客户端", rows: appState.snapshot.tools.prefix(5).map {
                    UsageDistributionRow(name: $0.tool, value: $0.tokens, percent: $0.percentValue, color: $0.displayColor)
                })
                distributionCard(title: "主力模型", rows: appState.snapshot.models.prefix(5).map {
                    UsageDistributionRow(name: $0.model, value: $0.tokens, percent: $0.percentValue, color: $0.displayColor)
                })
            }
        }
    }

    private var hero: some View {
        TokenCard {
            HStack(alignment: .center, spacing: 34) {
                ZStack {
                    ProgressRingView(progress: appState.progress, lineWidth: 20)
                    VStack(spacing: 6) {
                        Text(TokenStepFormat.tokens(appState.today.totalTokens))
                            .font(.system(size: 42, weight: .heavy, design: .rounded))
                            .foregroundStyle(Color.tokenInk)
                            .minimumScaleFactor(0.42)
                            .lineLimit(1)
                        Text("目标 \(TokenStepFormat.tokens(appState.settings.dailyGoalTokens, compact: true))")
                            .font(.headline.weight(.bold))
                            .foregroundStyle(.secondary)
                    }
                    .frame(width: 160)
                }
                .frame(width: 204, height: 204)

                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("今日完成")
                            .font(.headline.weight(.bold))
                            .foregroundStyle(.secondary)
                        Text(TokenStepFormat.percent(min(appState.progress * 100, 999)))
                            .font(.system(size: 56, weight: .heavy, design: .rounded))
                            .foregroundStyle(Color.tokenInk)
                            .monospacedDigit()
                        Text(progressSentence)
                            .font(.title3.weight(.bold))
                            .foregroundStyle(Color.tokenGreenDark)
                    }

                    HStack(spacing: 10) {
                        MetricPill(label: "消耗金额", value: TokenStepFormat.money(appState.today.cost))
                        MetricPill(label: "本月均值", value: TokenStepFormat.tokens(appState.monthAverage, compact: true))
                    }
                }

                Spacer(minLength: 0)
            }
        }
    }

    private var metricStrip: some View {
        HStack(spacing: 18) {
            CompactMetricCard(label: "累计 AI 步数", value: TokenStepFormat.tokens(appState.snapshot.totals.tokens), detail: "所有本机记录")
            CompactMetricCard(label: "活跃天数", value: "\(appState.snapshot.totals.activeDays) 天", detail: "有 AI 使用的日期")
            CompactMetricCard(label: "达标天数", value: "\(appState.goalDays) 天", detail: "达到每日目标")
        }
    }

    private func distributionCard(title: String, rows: [UsageDistributionRow]) -> some View {
        TokenCard {
            VStack(alignment: .leading, spacing: 17) {
                Text(title)
                    .font(.title3.weight(.heavy))
                    .foregroundStyle(Color.tokenInk)
                if rows.isEmpty {
                    Text("等待下一次同步")
                        .font(.callout.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 92, alignment: .leading)
                } else {
                    ForEach(rows) { row in
                        UsageProgressRow(
                            name: row.name,
                            value: "\(TokenStepFormat.tokens(row.value, compact: true)) · \(TokenStepFormat.percent(row.percent))",
                            percent: row.percent,
                            color: row.color
                        )
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var progressSentence: String {
        if appState.progress >= 1 {
            return "今天已经走满"
        }
        if appState.progress >= 0.65 {
            return "快到一个亿了"
        }
        if appState.progress >= 0.3 {
            return "节奏不错"
        }
        return "刚开始热身"
    }
}

private struct CompactMetricCard: View {
    var label: String
    var value: String
    var detail: String

    var body: some View {
        TokenCard {
            VStack(alignment: .leading, spacing: 8) {
                Text(label)
                    .font(.callout.weight(.bold))
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.system(size: 27, weight: .heavy, design: .rounded))
                    .foregroundStyle(Color.tokenInk)
                    .minimumScaleFactor(0.66)
                    .lineLimit(1)
                Text(detail)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct UsageDistributionRow: Identifiable {
    var id: String { name }
    var name: String
    var value: Int
    var percent: Double
    var color: Color
}
