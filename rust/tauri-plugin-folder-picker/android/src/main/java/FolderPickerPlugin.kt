package app.tauri.folderpicker

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.result.ActivityResult
import app.tauri.Logger
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class FolderPickerPlugin(private val activity: Activity) : Plugin(activity) {

    private var pendingInvoke: Invoke? = null

    @Command
    fun pickFolder(invoke: Invoke) {
        try {
            // On Android 11+ (API 30), check if we have MANAGE_EXTERNAL_STORAGE permission
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !Environment.isExternalStorageManager()) {
                // Save the invoke to resume after permission is granted
                pendingInvoke = invoke
                val intent = Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)
                startActivityForResult(invoke, intent, "permissionResult")
                return
            }

            launchFolderPicker(invoke)
        } catch (ex: Exception) {
            val message = ex.message ?: "Failed to open folder picker"
            Logger.error(message)
            invoke.reject(message)
        }
    }

    @ActivityCallback
    fun permissionResult(invoke: Invoke, result: ActivityResult) {
        try {
            // Check if permission was granted after returning from settings
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && Environment.isExternalStorageManager()) {
                launchFolderPicker(invoke)
            } else {
                // Permission denied, still try to launch the folder picker
                // The user can pick a folder with scoped access even without MANAGE_EXTERNAL_STORAGE
                launchFolderPicker(invoke)
            }
        } catch (ex: Exception) {
            val message = ex.message ?: "Failed to open folder picker after permission"
            Logger.error(message)
            invoke.reject(message)
        }
    }

    private fun launchFolderPicker(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        startActivityForResult(invoke, intent, "folderPickerResult")
    }

    @ActivityCallback
    fun folderPickerResult(invoke: Invoke, result: ActivityResult) {
        try {
            when (result.resultCode) {
                Activity.RESULT_OK -> {
                    val treeUri = result.data?.data
                    if (treeUri != null) {
                        val realPath = resolveTreeUriToPath(treeUri)
                        val ret = JSObject()
                        ret.put("path", realPath)
                        invoke.resolve(ret)
                    } else {
                        val ret = JSObject()
                        ret.put("path", null)
                        invoke.resolve(ret)
                    }
                }

                Activity.RESULT_CANCELED -> {
                    val ret = JSObject()
                    ret.put("path", null)
                    invoke.resolve(ret)
                }

                else -> {
                    val ret = JSObject()
                    ret.put("path", null)
                    invoke.resolve(ret)
                }
            }
        } catch (ex: Exception) {
            val message = ex.message ?: "Failed to process folder selection"
            Logger.error(message)
            invoke.reject(message)
        }
    }

    @Command
    fun restoreAccess(invoke: Invoke) {
        // No-op on Android — permissions are managed at the OS level
        invoke.resolve(JSObject())
    }

    /**
     * Resolves an ACTION_OPEN_DOCUMENT_TREE content URI to a real filesystem path.
     *
     * Tree URIs from the external storage documents provider follow the pattern:
     *   content://com.android.externalstorage.documents/tree/<storage>:<relative_path>
     *
     * For primary storage, <storage> is "primary" and maps to /storage/emulated/0/.
     * For downloads, the path is /storage/emulated/0/Download.
     */
    private fun resolveTreeUriToPath(treeUri: Uri): String? {
        val docId = DocumentsContract.getTreeDocumentId(treeUri)

        // External storage provider: "primary:path/to/folder" or "XXXX-XXXX:path"
        if (treeUri.authority == "com.android.externalstorage.documents") {
            val parts = docId.split(":", limit = 2)
            val storageId = parts[0]
            val relativePath = if (parts.size > 1) parts[1] else ""

            if (storageId == "primary") {
                val basePath = Environment.getExternalStorageDirectory().absolutePath
                return if (relativePath.isEmpty()) basePath
                else "$basePath/$relativePath"
            }

            // SD card or USB storage
            return "/storage/$storageId/$relativePath"
        }

        // Downloads provider
        if (treeUri.authority == "com.android.providers.downloads.documents") {
            return "${Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath}"
        }

        // Fallback: return the URI string if we can't resolve it
        return treeUri.toString()
    }
}
