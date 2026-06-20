import AppKit
import SwiftUI

@MainActor
final class MainWindowPresenter {
    static let shared = MainWindowPresenter()

    private var window: NSWindow?

    func show(appState: AppState) {
        let window = self.window ?? makeWindow(appState: appState)
        self.window = window

        NSApp.activate(ignoringOtherApps: true)
        closeTransientPanels(except: window)
        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
    }

    private func makeWindow(appState: AppState) -> NSWindow {
        let rootView = MainWindowView()
            .environmentObject(appState)
            .frame(minWidth: 1080, minHeight: 720)

        let controller = NSHostingController(rootView: rootView)
        let window = NSWindow(contentViewController: controller)
        window.title = "TokenStep"
        window.identifier = NSUserInterfaceItemIdentifier("main")
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
        window.titlebarSeparatorStyle = .none
        window.toolbarStyle = .unifiedCompact
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 1080, height: 720)
        window.maxSize = NSSize(width: 1440, height: 980)
        window.setContentSize(NSSize(width: 1240, height: 820))
        window.center()
        window.setFrameAutosaveName("TokenStepMainWindow.v2")
        return window
    }

    private func closeTransientPanels(except mainWindow: NSWindow) {
        for window in NSApp.windows where window !== mainWindow && window.title.isEmpty {
            window.close()
        }
    }
}
