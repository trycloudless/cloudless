import Tauri
import UIKit
import UniformTypeIdentifiers

@available(iOS 14.0, *)
class FolderPickerPlugin: Plugin {

    private static let bookmarkKey = "FolderPickerBookmarks"

    @objc public func pickFolder(_ invoke: Invoke) {
        DispatchQueue.main.async {
            let picker = UIDocumentPickerViewController(
                forOpeningContentTypes: [UTType.folder])
            picker.allowsMultipleSelection = false
            picker.modalPresentationStyle = .fullScreen

            let delegate = FolderPickerDelegate(invoke: invoke, plugin: self)
            // Retain the delegate for the lifetime of the picker
            objc_setAssociatedObject(picker, "delegate", delegate, .OBJC_ASSOCIATION_RETAIN)
            picker.delegate = delegate

            self.manager.viewController?.present(picker, animated: true, completion: nil)
        }
    }

    @objc public func restoreAccess(_ invoke: Invoke) {
        guard let bookmarks = UserDefaults.standard.dictionary(forKey: Self.bookmarkKey) else {
            invoke.resolve([:])
            return
        }

        for (_, value) in bookmarks {
            guard let bookmarkData = value as? Data else { continue }
            do {
                var isStale = false
                let url = try URL(
                    resolvingBookmarkData: bookmarkData,
                    options: [],
                    relativeTo: nil,
                    bookmarkDataIsStale: &isStale)
                if isStale {
                    saveBookmark(for: url)
                }
                _ = url.startAccessingSecurityScopedResource()
            } catch {
                // Bookmark no longer valid, skip
            }
        }

        invoke.resolve([:])
    }

    func handleFolderSelected(_ url: URL, invoke: Invoke) {
        _ = url.startAccessingSecurityScopedResource()
        saveBookmark(for: url)
        invoke.resolve(["path": url.path])
    }

    func handlePickerCancelled(invoke: Invoke) {
        invoke.resolve(["path": NSNull()])
    }

    private func saveBookmark(for url: URL) {
        do {
            let bookmarkData = try url.bookmarkData(
                options: [],
                includingResourceValuesForKeys: nil,
                relativeTo: nil)

            var bookmarks = UserDefaults.standard.dictionary(forKey: Self.bookmarkKey) ?? [:]
            bookmarks[url.path] = bookmarkData
            UserDefaults.standard.set(bookmarks, forKey: Self.bookmarkKey)
        } catch {
            // Failed to create bookmark, access may not persist across launches
        }
    }
}

@available(iOS 14.0, *)
class FolderPickerDelegate: NSObject, UIDocumentPickerDelegate {
    let invoke: Invoke
    let plugin: FolderPickerPlugin

    init(invoke: Invoke, plugin: FolderPickerPlugin) {
        self.invoke = invoke
        self.plugin = plugin
    }

    public func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard let url = urls.first else {
            plugin.handlePickerCancelled(invoke: invoke)
            return
        }
        plugin.handleFolderSelected(url, invoke: invoke)
    }

    public func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        plugin.handlePickerCancelled(invoke: invoke)
    }
}

@_cdecl("init_plugin_folder_picker")
func initPlugin() -> Plugin {
    if #available(iOS 14.0, *) {
        return FolderPickerPlugin()
    } else {
        fatalError("FolderPickerPlugin requires iOS 14.0 or later")
    }
}
