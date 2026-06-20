import SwiftUI

struct SettingsGoalCard: View {
    @EnvironmentObject private var appState: AppState

    var body: some View {
        SettingsCard(title: L("每日目标"), symbol: "figure.walk.circle.fill") {
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
                        Text(L("token / 天"))
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
}
