import Foundation
import SwiftUI

enum TokenStepFormat {
    static func tokens(_ value: Int, compact: Bool = false) -> String {
        if value >= 100_000_000 {
            return "\(trim(Double(value) / 100_000_000, digits: 2))亿"
        }
        if value >= 10_000 {
            let digits = compact || value >= 10_000_000 ? 0 : 1
            return "\(trim(Double(value) / 10_000, digits: digits))万"
        }
        return "\(value)"
    }

    static func money(_ value: Double) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.currencyCode = "USD"
        formatter.currencySymbol = "$"
        formatter.maximumFractionDigits = 2
        return formatter.string(from: NSNumber(value: value)) ?? "$0.00"
    }

    static func percent(_ value: Double) -> String {
        if value >= 100 { return "\(Int(value.rounded()))%" }
        if value >= 10 { return String(format: "%.1f%%", value) }
        return "\(Int(value.rounded()))%"
    }

    static func generatedTime(_ value: String?) -> String {
        guard let value, !value.isEmpty else { return "等待同步" }
        return value.replacingOccurrences(of: "T", with: " ").prefix(16).description
    }

    static func intervalLabel(_ seconds: Int) -> String {
        switch seconds {
        case 0: return "手动"
        case 60: return "1 分钟"
        default: return "\(seconds / 60) 分钟"
        }
    }

    private static func trim(_ value: Double, digits: Int) -> String {
        let text = String(format: "%.\(digits)f", value)
        return text.replacingOccurrences(of: #"(\.0+|(?<=\.\d)0+)$"#, with: "", options: .regularExpression)
    }
}

extension DateFormatter {
    static let tokenStepDay: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }()
}

extension Color {
    static let tokenInk = Color(red: 31 / 255, green: 41 / 255, blue: 55 / 255)
    static let tokenCanvas = Color(red: 247 / 255, green: 250 / 255, blue: 247 / 255)
    static let tokenSurface = Color(red: 255 / 255, green: 255 / 255, blue: 252 / 255)
    static let tokenGreen = Color(red: 45 / 255, green: 164 / 255, blue: 78 / 255)
    static let tokenGreenDark = Color(red: 33 / 255, green: 110 / 255, blue: 57 / 255)
    static let tokenMint = Color(red: 155 / 255, green: 233 / 255, blue: 168 / 255)
    static let tokenTrack = Color(red: 235 / 255, green: 237 / 255, blue: 240 / 255)
}

extension ToolUsage {
    var displayColor: Color {
        tool == "Claude Code" ? .tokenGreenDark : .tokenGreen
    }
}

extension ModelUsage {
    var displayColor: Color {
        tool == "Claude Code" ? .tokenGreenDark : .tokenGreen
    }
}
