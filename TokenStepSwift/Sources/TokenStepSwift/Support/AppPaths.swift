import Foundation

enum AppPaths {
    static let projectRoot = URL(fileURLWithPath: "/Users/superhuang/Documents/黄叔知识库/03-工具与效率/token-usage-monitor", isDirectory: true)
    static let usageJSON = projectRoot.appendingPathComponent("data/usage.json")
    static let settingsJSON = projectRoot.appendingPathComponent("config/settings.json")
    static let autostartDefaultMarker = projectRoot.appendingPathComponent("config/autostart-default-applied")
    static let collector = projectRoot.appendingPathComponent("token_usage_monitor.py")
    static let logs = projectRoot.appendingPathComponent("logs", isDirectory: true)
}
