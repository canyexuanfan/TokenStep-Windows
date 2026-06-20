import AppKit
import SwiftUI

struct PopoverPanelView: View {
    @EnvironmentObject private var appState: AppState
    @Environment(\.isScreenshotRendering) private var isScreenshotRendering

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            if let error = appState.lastError {
                ErrorBanner(message: error) {
                    appState.clearError()
                }
            }
            PopoverTodayRingCard()
            if appState.settings.showCodexQuota {
                PopoverQuotaCard()
            }
            trendCard
            if let update = appState.availableUpdate {
                UpdateNoticeCard(update: update)
            }
            PopoverFooterView()
        }
        .padding(20)
        .frame(width: 412)
        .background(TokenStepBackdrop())
        .id(appState.appearanceID)
    }

    private var header: some View {
        HStack(spacing: 13) {
            TokenStepMark(size: 42)
            VStack(alignment: .leading, spacing: 3) {
                Text("TokenStep")
                    .font(.system(size: 24, weight: .heavy, design: .rounded))
                    .foregroundStyle(Color.tokenInk)
                Text(L("每日 Token 消耗追踪"))
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            HStack(spacing: 6) {
                Circle()
                    .fill(appState.isRefreshing ? Color.secondary.opacity(0.68) : Color.tokenGreen)
                    .frame(width: 7, height: 7)
                Text(appState.isRefreshing ? L("同步中") : L("已同步"))
                    .font(.caption.weight(.heavy))
                    .foregroundStyle(Color.tokenInk.opacity(0.72))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(Color.tokenSurface, in: Capsule())
            .overlay(Capsule().stroke(Color.black.opacity(0.055)))

            if !isScreenshotRendering {
                ScreenshotMenuButton(
                    copyTitle: L("复制浮层截图"),
                    saveTitle: L("保存浮层 PNG"),
                    help: L("截取浮层"),
                    copyAction: copyPopoverScreenshot,
                    saveAction: savePopoverScreenshot
                )
            }
        }
    }

    private var trendCard: some View {
        TokenCard {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text(L("最近 30 天"))
                        .font(.headline.weight(.heavy))
                        .foregroundStyle(Color.tokenInk)
                    Spacer()
                    Text(L("细线是每日目标"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                ActivityBarsView(rows: appState.snapshot.daily, goal: appState.settings.dailyGoalTokens)
                    .frame(height: 66)
            }
        }
    }

    private var popoverScreenshot: some View {
        PopoverPanelView()
            .environmentObject(appState)
            .environment(\.isScreenshotRendering, true)
    }

    private func copyPopoverScreenshot() {
        do {
            try ScreenshotExporter.copy(popoverScreenshot)
        } catch {
            appState.lastError = error.localizedDescription
        }
    }

    private func savePopoverScreenshot() {
        do {
            try ScreenshotExporter.save(
                popoverScreenshot,
                suggestedFileName: ScreenshotExporter.suggestedFileName(prefix: "popover")
            )
        } catch {
            appState.lastError = error.localizedDescription
        }
    }

}

private struct UpdateNoticeCard: View {
    @EnvironmentObject private var appState: AppState
    var update: AvailableUpdate

    var body: some View {
        HStack(alignment: .center, spacing: 13) {
            Image(systemName: "arrow.down.circle.fill")
                .font(.system(size: 24, weight: .heavy))
                .foregroundStyle(Color.tokenGreen)
                .frame(width: 38, height: 38)
                .background(Color.tokenMint.opacity(0.22), in: Circle())

            VStack(alignment: .leading, spacing: 4) {
                Text(LFormat("发现新版本 %@", update.version))
                    .font(.callout.weight(.heavy))
                    .foregroundStyle(Color.tokenInk)
                Text(update.noteLines.first ?? L("内存占用优化与稳定性改进"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            Button {
                appState.showUpdateDetails()
            } label: {
                Text(L("立即更新"))
                    .font(.caption.weight(.heavy))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(Color.tokenGreen, in: Capsule())
            }
            .buttonStyle(.plain)

            Button {
                appState.postponeUpdateNotice()
            } label: {
                Text(L("稍后"))
                    .font(.caption.weight(.heavy))
                    .foregroundStyle(Color.tokenInk.opacity(0.64))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .background(Color.tokenTrack.opacity(0.42), in: Capsule())
            }
            .buttonStyle(.plain)
        }
        .padding(13)
        .background(Color.tokenSurface, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.tokenGreen.opacity(0.14)))
        .shadow(color: Color.black.opacity(0.045), radius: 12, x: 0, y: 7)
    }
}
