package pl.mkrstudio.truefootball3.activities

import android.app.Activity
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.provider.DocumentsContract
import android.view.Gravity
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import java.util.Locale
import pl.mkrstudio.truefootball3.R

class AudioFilePickerActivity : Activity() {
    private data class Entry(
        val documentId: String,
        val name: String,
        val mimeType: String?,
        val size: Long
    ) {
        val isDirectory: Boolean get() = DocumentsContract.Document.MIME_TYPE_DIR == mimeType
    }

    private val entries = ArrayList<Entry>()
    private val folderStack = ArrayList<Uri>()
    private lateinit var prefs: SharedPreferences
    private var treeUri: Uri? = null
    private var currentFolderUri: Uri? = null
    private lateinit var folderTitle: TextView
    private lateinit var recentFile: TextView
    private lateinit var fileList: ListView
    private lateinit var upButton: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestWindowFeature(1)
        prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        buildUi()
        restoreLastLocation()
    }

    private fun buildUi() {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(10), dp(12), dp(10), dp(10))
            setBackgroundResource(R.drawable.main_background)
        }

        val title = text("Quản lý file âm thanh", 22, Color.WHITE).apply { gravity = Gravity.CENTER }
        root.addView(title, fullWrap())

        folderTitle = text("", 16, 0xffd8ebff.toInt()).apply {
            gravity = Gravity.CENTER
            setPadding(dp(4), dp(8), dp(4), dp(4))
        }
        root.addView(folderTitle, fullWrap())

        recentFile = text("", 13, 0xffb7d9ef.toInt()).apply {
            gravity = Gravity.CENTER
            setPadding(dp(4), 0, dp(4), dp(8))
        }
        root.addView(recentFile, fullWrap())

        fileList = ListView(this).apply {
            setOnItemClickListener { _, _, position, _ ->
                entries.getOrNull(position)?.let { openEntry(it) }
            }
        }
        root.addView(fileList, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))

        val actions = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        upButton = button("Lên thư mục") { goUp() }
        actions.addView(upButton, weighted())
        actions.addView(button("Đổi thư mục gốc") { chooseRoot() }, weighted())
        actions.addView(button("Hủy") { finish() }, weighted())
        root.addView(actions, fullWrap())
        setContentView(root)
    }

    private fun restoreLastLocation() {
        val savedTree = prefs.getString("tree_uri", null)
        if (savedTree == null) {
            folderTitle.text = "Hãy chọn thư mục chứa âm thanh"
            chooseRoot()
            return
        }
        try {
            val parsedTree = Uri.parse(savedTree)
            treeUri = parsedTree
            val rootId = DocumentsContract.getTreeDocumentId(parsedTree)
            val rootDocument = DocumentsContract.buildDocumentUriUsingTree(parsedTree, rootId)
            folderStack.clear()
            folderStack.add(rootDocument)
            val savedFolderId = prefs.getString("folder_document_id", rootId) ?: rootId
            val current = DocumentsContract.buildDocumentUriUsingTree(parsedTree, savedFolderId)
            currentFolderUri = current
            if (rootDocument != current) folderStack.add(current)
            loadDirectory()
        } catch (_: RuntimeException) {
            clearSavedLocation()
            chooseRoot()
        }
    }

    @Suppress("DEPRECATION")
    private fun chooseRoot() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            treeUri?.let { putExtra(EXTRA_INITIAL_URI, it) }
        }
        startActivityForResult(intent, SELECT_ROOT)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != SELECT_ROOT || resultCode != RESULT_OK || data?.data == null) {
            if (treeUri == null) folderTitle.text = "Chưa chọn thư mục âm thanh"
            return
        }
        val selectedTree = data.data ?: return
        try {
            contentResolver.takePersistableUriPermission(selectedTree, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            treeUri = selectedTree
            val rootId = DocumentsContract.getTreeDocumentId(selectedTree)
            val rootDoc = DocumentsContract.buildDocumentUriUsingTree(selectedTree, rootId)
            currentFolderUri = rootDoc
            folderStack.clear()
            folderStack.add(rootDoc)
            prefs.edit()
                .putString("tree_uri", selectedTree.toString())
                .putString("folder_document_id", rootId)
                .remove("last_file_document_id")
                .remove("last_file_name")
                .apply()
            loadDirectory()
        } catch (_: SecurityException) {
            Toast.makeText(this, "Không thể giữ quyền đọc thư mục này", Toast.LENGTH_LONG).show()
        }
    }

    private fun loadDirectory() {
        entries.clear()
        val tree = treeUri
        val current = currentFolderUri
        if (tree == null || current == null) {
            clearSavedLocation()
            chooseRoot()
            return
        }
        try {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(
                tree,
                DocumentsContract.getDocumentId(current)
            )
            val cursor = contentResolver.query(
                children,
                arrayOf(
                    DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                    DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                    DocumentsContract.Document.COLUMN_MIME_TYPE,
                    DocumentsContract.Document.COLUMN_SIZE
                ),
                null,
                null,
                null
            ) ?: throw SecurityException("No cursor")
            cursor.use { c ->
                while (c.moveToNext()) {
                    val id = c.getString(0) ?: continue
                    val name = c.getString(1)
                    val mime = c.getString(2)
                    val size = if (c.isNull(3)) 0L else c.getLong(3)
                    if (DocumentsContract.Document.MIME_TYPE_DIR == mime || isAudio(name, mime)) {
                        entries.add(Entry(id, name ?: "Không tên", mime, size))
                    }
                }
            }
            entries.sortWith { left, right ->
                if (left.isDirectory != right.isDirectory) {
                    if (left.isDirectory) -1 else 1
                } else {
                    left.name.compareTo(right.name, ignoreCase = true)
                }
            }
            showEntries()
            prefs.edit().putString("folder_document_id", DocumentsContract.getDocumentId(current)).apply()
        } catch (_: RuntimeException) {
            Toast.makeText(this, "Thư mục cũ không còn truy cập được. Hãy chọn lại.", Toast.LENGTH_LONG).show()
            clearSavedLocation()
            chooseRoot()
        }
    }

    private fun showEntries() {
        val lastId = prefs.getString("last_file_document_id", null)
        val lastName = prefs.getString("last_file_name", null)
        val rows = ArrayList<String>()
        var recentPosition = -1
        for (i in entries.indices) {
            val entry = entries[i]
            if (entry.isDirectory) {
                rows.add("[Thư mục]  ${entry.name}")
            } else {
                val recent = entry.documentId == lastId
                rows.add((if (recent) "✓ Gần nhất  " else "") + entry.name + formatSize(entry.size))
                if (recent) recentPosition = i
            }
        }
        if (rows.isEmpty()) rows.add("Không có tệp âm thanh trong thư mục này")
        fileList.adapter = ArrayAdapter(this, android.R.layout.simple_list_item_1, rows)
        fileList.isEnabled = entries.isNotEmpty()
        folderTitle.text = "Thư mục hiện tại: ${currentFolderUri?.let { queryDisplayName(it) } ?: "Âm thanh"}"
        recentFile.text = if (lastName == null) "" else "Tệp đã chọn gần nhất: $lastName"
        upButton.isEnabled = folderStack.size > 1
        if (recentPosition >= 0) fileList.setSelection(recentPosition)
    }

    private fun openEntry(entry: Entry) {
        val tree = treeUri ?: return
        val documentUri = DocumentsContract.buildDocumentUriUsingTree(tree, entry.documentId)
        if (entry.isDirectory) {
            currentFolderUri = documentUri
            folderStack.add(documentUri)
            loadDirectory()
            return
        }
        currentFolderUri?.let { current ->
            prefs.edit()
                .putString("folder_document_id", DocumentsContract.getDocumentId(current))
                .putString("last_file_document_id", entry.documentId)
                .putString("last_file_name", entry.name)
                .apply()
        }
        val result = Intent().apply {
            data = documentUri
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        setResult(RESULT_OK, result)
        finish()
    }

    private fun goUp() {
        if (folderStack.size <= 1) return
        folderStack.removeAt(folderStack.size - 1)
        currentFolderUri = folderStack.last()
        loadDirectory()
    }

    private fun queryDisplayName(uri: Uri): String {
        try {
            contentResolver.query(uri, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) return c.getString(0) ?: "Âm thanh"
            }
        } catch (_: RuntimeException) {
        }
        return "Âm thanh"
    }

    private fun clearSavedLocation() {
        prefs.edit().remove("tree_uri").remove("folder_document_id").apply()
        treeUri = null
        currentFolderUri = null
        folderStack.clear()
    }

    private fun text(value: String, sp: Int, color: Int): TextView = TextView(this).apply {
        text = value
        textSize = sp.toFloat()
        setTextColor(color)
    }

    private fun button(value: String, onClick: () -> Unit): Button = Button(this).apply {
        text = value
        textSize = 12f
        setOnClickListener { onClick() }
    }

    private fun fullWrap(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)

    private fun weighted(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density + 0.5f).toInt()

    companion object {
        private const val SELECT_ROOT = 7201
        private const val PREFS = "tf3AudioPicker"
        private const val EXTRA_INITIAL_URI = "android.provider.extra.INITIAL_URI"

        private fun isAudio(name: String?, mime: String?): Boolean {
            if (mime != null && mime.lowercase(Locale.US).startsWith("audio/")) return true
            if (name == null) return false
            val lower = name.lowercase(Locale.US)
            return lower.endsWith(".mp3") || lower.endsWith(".ogg") || lower.endsWith(".wav") ||
                lower.endsWith(".m4a") || lower.endsWith(".aac") || lower.endsWith(".flac") ||
                lower.endsWith(".opus") || lower.endsWith(".3gp") || lower.endsWith(".mp4")
        }

        private fun formatSize(bytes: Long): String {
            if (bytes <= 0L) return ""
            if (bytes < 1024L * 1024L) return String.format(Locale.US, "  (%.0f KB)", bytes / 1024f)
            return String.format(Locale.US, "  (%.1f MB)", bytes / (1024f * 1024f))
        }
    }
}
