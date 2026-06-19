import Foundation

@MainActor
final class AppState: ObservableObject {
    @Published private(set) var snapshot: UsageSnapshot = .empty
    @Published private(set) var settings: TokenStepSettings = .defaults
    @Published private(set) var isRefreshing = false
    @Published private(set) var autostartEnabled = false
    @Published var lastError: String?

    private var timer: Timer?

    init() {
        load()
        applyDefaultAutostartIfNeeded()
        configureTimer()
        refresh()
    }

    deinit {
        timer?.invalidate()
    }

    var today: DailyUsage {
        let key = DateFormatter.tokenStepDay.string(from: Date())
        return snapshot.daily.last(where: { $0.date == key })
            ?? snapshot.daily.last
            ?? DailyUsage(date: key, tools: [:], totalTokens: 0, cost: 0)
    }

    var progress: Double {
        guard settings.dailyGoalTokens > 0 else { return 0 }
        return Double(today.totalTokens) / Double(settings.dailyGoalTokens)
    }

    var monthAverage: Int {
        let rows = Array(snapshot.daily.suffix(30))
        guard !rows.isEmpty else { return 0 }
        return rows.map(\.totalTokens).reduce(0, +) / rows.count
    }

    var goalDays: Int {
        snapshot.daily.filter { $0.totalTokens >= settings.dailyGoalTokens }.count
    }

    var visibleHistoryRows: [DailyUsage] {
        Array(snapshot.daily.reversed())
    }

    func load() {
        settings = DataService.loadSettings()
        snapshot = (try? DataService.loadSnapshot()) ?? .empty
        autostartEnabled = AutostartService.isEnabled
    }

    func refresh() {
        guard !isRefreshing else { return }
        isRefreshing = true
        lastError = nil
        Task {
            do {
                try await Task.detached(priority: .utility) {
                    try DataService.runCollector()
                }.value
            } catch {
                lastError = error.localizedDescription
            }
            load()
            isRefreshing = false
        }
    }

    func clearError() {
        lastError = nil
    }

    func setGoal(_ tokens: Int) {
        settings.dailyGoalTokens = max(1_000_000, tokens)
        saveSettingsAndReload()
    }

    func setRefreshInterval(_ seconds: Int) {
        settings.refreshIntervalSeconds = seconds
        saveSettingsAndReload()
        configureTimer()
    }

    func setAutostart(_ enabled: Bool) {
        do {
            try AutostartService.setEnabled(enabled)
            try markAutostartDefaultApplied()
            autostartEnabled = AutostartService.isEnabled
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func saveSettingsAndReload() {
        do {
            try DataService.saveSettings(settings)
            settings = DataService.loadSettings()
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func configureTimer() {
        timer?.invalidate()
        timer = nil
        guard settings.refreshIntervalSeconds > 0 else { return }
        timer = Timer.scheduledTimer(withTimeInterval: TimeInterval(settings.refreshIntervalSeconds), repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.load()
                self?.refresh()
            }
        }
    }

    private func applyDefaultAutostartIfNeeded() {
        guard !FileManager.default.fileExists(atPath: AppPaths.autostartDefaultMarker.path) else { return }
        do {
            if !AutostartService.isEnabled {
                try AutostartService.setEnabled(true)
            }
            try markAutostartDefaultApplied()
            autostartEnabled = AutostartService.isEnabled
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func markAutostartDefaultApplied() throws {
        try FileManager.default.createDirectory(
            at: AppPaths.autostartDefaultMarker.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("applied\n".utf8).write(to: AppPaths.autostartDefaultMarker, options: .atomic)
    }
}
