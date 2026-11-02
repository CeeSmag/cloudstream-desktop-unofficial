/** Jsoup-shaped proxy; the real DOM lives in the Rust host. */
class Element(private val handle: Int) {
    fun text(): String = readUtf8(65536) { p, c -> hostText(handle, p, c) } ?: ""

    fun attr(name: String): String =
        withUtf8(name) { np, nl -> readUtf8(8192) { p, c -> hostAttr(handle, np, nl, p, c) } } ?: ""

    fun select(selector: String): List<Element> {
        val list = withUtf8(selector) { p, l -> hostSelect(handle, p, l) }
        return List(hostListLen(list)) { Element(hostListItem(list, it)) }
    }
}

class HostRequestFailed(message: String) : Exception(message)

fun httpGetDocument(url: String): Element {
    val handle = withUtf8(url) { p, l -> hostHttpGetDocument(p, l) }
    if (handle < 0) throw HostRequestFailed("host refused request for $url")
    return Element(handle)
}
