import AppKit
import SwiftUI

struct TokenIslandWindowView: View {
    @EnvironmentObject private var appState: AppState

    var onHoverChanged: (Bool) -> Void
    var onTap: () -> Void

    var body: some View {
        TokenIslandRingView(
            tokens: appState.today.totalTokens,
            lap: appState.todayLap,
            refreshing: appState.isRefreshing,
            theme: appState.settings.theme,
            language: appState.settings.language
        )
        .onHover { hovering in
            onHoverChanged(hovering)
        }
        .onTapGesture {
            onTap()
        }
        .id(appState.appearanceID)
    }
}

struct TokenIslandPopoverWindowView: View {
    @EnvironmentObject private var appState: AppState

    var onHoverChanged: (Bool) -> Void

    var body: some View {
        PopoverPanelView()
            .environmentObject(appState)
            .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 26, style: .continuous)
                    .stroke(Color.black.opacity(0.08))
            )
            .shadow(color: Color.black.opacity(0.18), radius: 28, x: 0, y: 16)
            .onHover { hovering in
                onHoverChanged(hovering)
            }
            .transition(.opacity.combined(with: .scale(scale: 0.98, anchor: .topLeading)))
            .id(appState.appearanceID)
            .animation(.spring(response: 0.26, dampingFraction: 0.88), value: appState.appearanceID)
    }
}

struct TokenIslandRingView: View {
    var tokens: Int
    var lap: TokenStepLapProgress
    var refreshing: Bool
    var theme: TokenStepTheme
    var language: TokenStepLanguage

    var body: some View {
        HStack(spacing: 5) {
            Image(nsImage: StatusBarIconRenderer.progressRing(
                progress: lap.currentLapProgress,
                lap: lap.currentLap,
                refreshing: refreshing,
                size: 16,
                radius: 6.2,
                lineWidth: 2.15,
                showsCenterDot: false
            ))
                .resizable()
                .interpolation(.high)
                .frame(width: 15, height: 15)
                .accessibilityLabel("\(lap.lapTitle) \(lap.lapPercentText)")
                .id("\(theme.id)-\(language.resolved.id)")

            Text(TokenStepFormat.tokens(tokens, compact: true, language: language))
                .font(.system(size: 13, weight: .heavy, design: .rounded))
                .foregroundStyle(.white)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.74)
        }
        .padding(.leading, 7)
        .padding(.trailing, 8)
        .frame(width: TokenIslandWindowPresenter.collapsedSize.width, height: TokenIslandWindowPresenter.collapsedSize.height)
        .background(Color.black)
        .clipShape(Capsule())
        .id("\(theme.id)-\(language.resolved.id)")
    }
}
