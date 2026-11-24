package dev.n0bridge.n0te

/**
 * How the library is organised, without any screen: folders nest with "/" ("Uni/WS26"), tags
 * are names with a colour of their own, notebooks can be pinned. The same rules as the laptop.
 */
object LibraryOrg {
    /** Where the library looks: everything, the pinned ones, a folder, a tag. */
    sealed class Place {
        /** The start: recently opened, pinned, the folders, what is in none. */
        object Home : Place()
        object All : Place()
        object Pinned : Place()
        data class Folder(val path: String) : Place()
        data class Tag(val name: String) : Place()
    }

    /** A folder in the sidebar: its path, name, depth, how many notebooks (with its subfolders'), whether it has subfolders. */
    data class Node(val path: String, val name: String, val depth: Int, val count: Int, val hasChildren: Boolean)

    /** The tag colours (the pen's inks and a few more), the same on the laptop. */
    val TAG_COLORS = longArrayOf(0xFF1E5BD8, 0xFFD71921, 0xFF138A47, 0xFFE08A00, 0xFF7B3FD0, 0xFF0AA4B8, 0xFF8C6239, 0xFFAD1457)

    /** A tag's colour: always the same for the same name (ignoring case). */
    fun tagColor(tag: String): Long = TAG_COLORS[tag.lowercase().sumOf { it.code } % TAG_COLORS.size]

    fun parent(path: String) = path.substringBeforeLast('/', "")
    fun name(path: String) = path.substringAfterLast('/')

    /** A folder path tidied: parts trimmed, empty ones left out (like the core). */
    fun clean(path: String) = path.split('/').map { it.trim() }.filter { it.isNotEmpty() }.joinToString("/")

    /** Every folder (with the ones above it) of [folders] and [extra] (made but still empty), sorted. */
    fun allFolders(folders: Collection<String>, extra: Collection<String> = emptyList()): List<String> {
        val out = sortedSetOf(compareBy<String>({ it.lowercase() }, { it }))
        for (f in folders + extra) {
            var p = clean(f)
            while (p.isNotEmpty()) { out += p; p = parent(p) }
        }
        return out.toList()
    }

    /**
     * The folder tree for the sidebar: depth first, children under their parent, sorted by name;
     * [collapsed] folders' children left out. Counts include the subfolders.
     */
    fun tree(folders: Collection<String>, extra: Collection<String> = emptyList(), collapsed: Set<String> = emptySet()): List<Node> {
        val all = allFolders(folders, extra)
        val notebooks = folders.map { clean(it) }
        fun count(p: String) = notebooks.count { it == p || it.startsWith("$p/") }
        val out = ArrayList<Node>()
        fun walk(parent: String, depth: Int) {
            val kids = all.filter { parent(it) == parent }.sortedWith(compareBy({ name(it).lowercase() }, { it }))
            for (k in kids) {
                val sub = all.any { parent(it) == k }
                out += Node(k, name(k), depth, count(k), sub)
                if (k !in collapsed) walk(k, depth + 1)
            }
        }
        walk("", 0)
        return out
    }

    /** A notebook's folder after folder [old] is renamed [new] (null: not in it). Its subfolders go along. */
    fun renamed(folder: String, old: String, new: String): String? = when {
        folder == old -> clean(new)
        folder.startsWith("$old/") -> clean(new + folder.removePrefix(old))
        else -> null
    }

    /** A notebook's folder after folder [path] is dissolved: everything in it one level up (null: not in it). */
    fun dissolved(folder: String, path: String): String? = when {
        folder == path -> parent(path)
        folder.startsWith("$path/") -> clean(parent(path) + "/" + folder.removePrefix("$path/"))
        else -> null
    }

    /** When notebooks were opened last, from what the settings keep ("id:ms;id:ms"). */
    fun opened(stored: String): Map<String, Long> =
        stored.split(';').mapNotNull { p -> p.split(':').takeIf { it.size == 2 }?.let { (id, t) -> t.toLongOrNull()?.let { id to it } } }.toMap()

    /** The same with [id] opened at [at], kept to the [keep] latest. */
    fun openedNow(stored: String, id: String, at: Long, keep: Int = 40): String =
        (opened(stored) + (id to at)).entries.sortedByDescending { it.value }.take(keep).joinToString(";") { "${it.key}:${it.value}" }

    private fun plain(s: String) = s.lowercase().replace("ä", "ae").replace("ö", "oe").replace("ü", "ue").replace("ß", "ss")

    /** Whether a notebook matches what was typed into the search (title, folder, tags; case and umlauts don't matter). */
    fun matches(title: String, folder: String, tags: List<String>, query: String): Boolean {
        val q = plain(query.trim())
        if (q.isEmpty()) return true
        return q.split(' ').filter { it.isNotEmpty() }.all { w -> plain(title).contains(w) || plain(folder).contains(w) || tags.any { plain(it).contains(w) } }
    }
}
