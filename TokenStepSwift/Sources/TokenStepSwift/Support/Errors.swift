import Foundation

enum TokenStepError: LocalizedError {
    case collectorFailed(status: Int32, message: String)

    var errorDescription: String? {
        switch self {
        case let .collectorFailed(status, message):
            if message.isEmpty {
                return "采集脚本退出码 \(status)"
            }
            return "采集脚本退出码 \(status)：\(message)"
        }
    }
}
