package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Test

/** The graph keyboard's editing: insert at the cursor, templates, brackets in pairs. */
class MathFieldTest {
    @Test fun typingWorksLikeGeoGebrasInput() {
        val f = MathField()
        f.insert("f(x) = x")
        f.insert("²")
        assertEquals("f(x) = x²", f.text)
        // √ brings its brackets, the cursor inside; ")" steps over the closing one.
        f.insert(" + √(", ")")
        f.insert("x")
        f.close()
        f.insert(" − 1")
        assertEquals("f(x) = x² + √(x) − 1", f.text)
        assertEquals(f.text.length, f.value.selection.start)
        // Backspace: one character; an empty pair of brackets at once.
        f.backspace()
        assertEquals("f(x) = x² + √(x) − ", f.text)
        f.open()
        assertEquals("f(x) = x² + √(x) − ()", f.text)
        f.backspace()
        assertEquals("f(x) = x² + √(x) − ", f.text)
        // Arrows move the cursor; typing goes there.
        repeat(3) { f.left() }
        f.insert("2")
        assertEquals("f(x) = x² + √(x)2 − ", f.text)
        // A template selects its arguments, to be typed over.
        val g = MathField()
        g.template("Schneide(f, g)")
        assertEquals("f, g" , g.text.substring(g.value.selection.min, g.value.selection.max))
        g.insert("f, h")
        assertEquals("Schneide(f, h)", g.text)
        // Enter runs what the field is for.
        var entered = ""
        g.onEnter = { entered = g.text }
        g.onEnter()
        assertEquals("Schneide(f, h)", entered)
    }
    @Test fun aPluggedInKeyboardTypesSelectsAndDeletes() {
        val f = MathField()
        "f(x)=\\frac{1".forEach { f.type(it) }
        // Brackets and braces come in pairs; the cursor sits inside.
        assertEquals("f(x)=\\frac{1}", f.text)
        f.type('}')
        f.type('{'); f.type('x'); f.type('}')
        assertEquals("f(x)=\\frac{1}{x}", f.text)
        assertEquals(f.text.length, f.value.selection.start)
        // Shift + arrows select, typing replaces the selection.
        f.left(); f.extend(-1)
        assertEquals("x", f.selected())
        f.type('y')
        assertEquals("f(x)=\\frac{1}{y}", f.text)
        // Delete takes the character after the cursor; Home/End jump.
        f.home()
        f.deleteForward()
        assertEquals("(x)=\\frac{1}{y}", f.text)
        f.end(select = false)
        assertEquals(f.text.length, f.value.selection.start)
        f.selectAll()
        assertEquals(f.text, f.selected())
    }
}
