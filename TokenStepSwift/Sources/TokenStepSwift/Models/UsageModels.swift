import Foundation

struct UsageSnapshot: Codable {
    var generatedAt: String?
    var timezone: String?
    var totals: UsageTotals
    var daily: [DailyUsage]
    var tools: [ToolUsage]
    var models: [ModelUsage]
    var sources: [String: SourceInfo]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case timezone
        case totals
        case daily
        case tools
        case models
        case sources
    }

    static let empty = UsageSnapshot(
        generatedAt: nil,
        timezone: "Asia/Shanghai",
        totals: UsageTotals(tokens: 0, cost: 0, activeDays: 0),
        daily: [],
        tools: [],
        models: [],
        sources: [:]
    )
}

struct UsageTotals: Codable {
    var tokens: Int
    var cost: Double
    var activeDays: Int

    enum CodingKeys: String, CodingKey {
        case tokens
        case cost
        case activeDays = "active_days"
    }
}

struct DailyUsage: Codable, Identifiable {
    var id: String { date }
    var date: String
    var tools: [String: Int]
    var totalTokens: Int
    var cost: Double

    enum CodingKeys: String, CodingKey {
        case date
        case tools
        case totalTokens = "total_tokens"
        case cost
    }
}

struct ToolUsage: Codable, Identifiable {
    var id: String { tool }
    var tool: String
    var tokens: Int
    var percent: Double?

    var percentValue: Double { percent ?? 0 }
}

struct ModelUsage: Codable, Identifiable {
    var id: String { "\(model)-\(tool ?? "")" }
    var model: String
    var tool: String?
    var tokens: Int
    var percent: Double?

    var percentValue: Double { percent ?? 0 }
}

struct SourceInfo: Codable {
    var status: String?
    var files: Int?
    var records: Int?
}

struct TokenStepSettings: Codable {
    var dailyGoalTokens: Int
    var refreshIntervalSeconds: Int
    var historyDays: Int
    var theme: TokenStepTheme
    var autoUpdateEnabled: Bool
    var askBeforeDownloadingUpdates: Bool
    var requireVerifiedUpdates: Bool
    var skippedUpdateVersion: String?

    enum CodingKeys: String, CodingKey {
        case dailyGoalTokens = "daily_goal_tokens"
        case refreshIntervalSeconds = "refresh_interval_seconds"
        case historyDays = "history_days"
        case theme
        case autoUpdateEnabled = "auto_update_enabled"
        case askBeforeDownloadingUpdates = "ask_before_downloading_updates"
        case requireVerifiedUpdates = "require_verified_updates"
        case skippedUpdateVersion = "skipped_update_version"
    }

    static let defaults = TokenStepSettings(
        dailyGoalTokens: 100_000_000,
        refreshIntervalSeconds: 60,
        historyDays: 180,
        theme: .green,
        autoUpdateEnabled: true,
        askBeforeDownloadingUpdates: true,
        requireVerifiedUpdates: true,
        skippedUpdateVersion: nil
    )

    init(
        dailyGoalTokens: Int,
        refreshIntervalSeconds: Int,
        historyDays: Int,
        theme: TokenStepTheme,
        autoUpdateEnabled: Bool,
        askBeforeDownloadingUpdates: Bool,
        requireVerifiedUpdates: Bool,
        skippedUpdateVersion: String?
    ) {
        self.dailyGoalTokens = dailyGoalTokens
        self.refreshIntervalSeconds = refreshIntervalSeconds
        self.historyDays = historyDays
        self.theme = theme
        self.autoUpdateEnabled = autoUpdateEnabled
        self.askBeforeDownloadingUpdates = askBeforeDownloadingUpdates
        self.requireVerifiedUpdates = requireVerifiedUpdates
        self.skippedUpdateVersion = skippedUpdateVersion
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = TokenStepSettings.defaults
        dailyGoalTokens = try container.decodeIfPresent(Int.self, forKey: .dailyGoalTokens) ?? defaults.dailyGoalTokens
        refreshIntervalSeconds = try container.decodeIfPresent(Int.self, forKey: .refreshIntervalSeconds) ?? defaults.refreshIntervalSeconds
        historyDays = try container.decodeIfPresent(Int.self, forKey: .historyDays) ?? defaults.historyDays
        let themeID = try container.decodeIfPresent(String.self, forKey: .theme)
        theme = themeID.flatMap(TokenStepTheme.init(rawValue:)) ?? defaults.theme
        autoUpdateEnabled = try container.decodeIfPresent(Bool.self, forKey: .autoUpdateEnabled) ?? defaults.autoUpdateEnabled
        askBeforeDownloadingUpdates = try container.decodeIfPresent(Bool.self, forKey: .askBeforeDownloadingUpdates) ?? defaults.askBeforeDownloadingUpdates
        requireVerifiedUpdates = try container.decodeIfPresent(Bool.self, forKey: .requireVerifiedUpdates) ?? defaults.requireVerifiedUpdates
        skippedUpdateVersion = try container.decodeIfPresent(String.self, forKey: .skippedUpdateVersion)
    }
}
