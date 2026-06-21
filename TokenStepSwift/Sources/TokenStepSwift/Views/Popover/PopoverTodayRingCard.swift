import SwiftUI

struct PopoverTodayRingCard: View {
    @EnvironmentObject private var appState: AppState

    var body: some View {
        let lap = appState.todayLap
        return TokenCard {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text(L("今日 Token 消耗"))
                        .font(.headline.weight(.heavy))
                        .foregroundStyle(Color.tokenInk)
                    Spacer()
                    Text(appState.today.date)
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.secondary)
                }

                HStack(spacing: 20) {
                    ZStack {
                        ProgressRingView(progress: lap.currentLapProgress, lineWidth: 16, color: lap.color)
                        VStack(spacing: 3) {
                            Text(TokenStepFormat.tokens(appState.today.totalTokens))
                                .font(.system(size: 31, weight: .heavy, design: .rounded))
                                .foregroundStyle(Color.tokenInk)
                                .minimumScaleFactor(0.52)
                                .lineLimit(1)
                            Text(LFormat("/ %@ 每圈", TokenStepFormat.tokens(appState.settings.dailyGoalTokens, compact: true)))
                                .font(.callout.weight(.bold))
                                .foregroundStyle(.secondary)
                        }
                        .frame(width: 122)
                    }
                    .frame(width: 148, height: 148)

                    VStack(alignment: .leading, spacing: 11) {
                        Text(lap.lapTitle)
                            .font(.headline.weight(.heavy))
                            .foregroundStyle(Color.tokenInk)
                        Text(lap.lapPercentText)
                            .font(.system(size: 43, weight: .heavy, design: .rounded))
                            .foregroundStyle(lap.color)
                            .monospacedDigit()
                        Text(lap.completedLapsText)
                            .font(.headline.weight(.bold))
                            .foregroundStyle(.secondary)

                        VStack(alignment: .leading, spacing: 8) {
                            MetricPill(label: L("消耗金额"), value: TokenStepFormat.money(appState.today.cost))
                            MetricPill(label: L("活跃"), value: localizedDays(appState.snapshot.totals.activeDays))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }

                if let summary = todayToolSummary {
                    Text(summary)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.top, 1)
                }
            }
        }
    }

    private func localizedDays(_ count: Int) -> String {
        TokenStepLocalization.language == .en ? "\(count)d" : "\(count) 天"
    }

    private var todayToolSummary: String? {
        guard appState.today.totalTokens > 0 else { return nil }
        let orderedTools = [("Codex", "Codex"), ("Claude Code", "Claude")]
        let parts = orderedTools.map { tool, label in
            "\(label) \(TokenStepFormat.tokens(appState.today.tools[tool] ?? 0, compact: true))"
        }
        return "\(L("今日")) \(parts.joined(separator: " · "))"
    }
}
