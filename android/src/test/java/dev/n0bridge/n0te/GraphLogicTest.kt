package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Test

/** The graph tools' commands and numbers as the core reads them. */
class GraphLogicTest {
    @Test fun numbersNeverComeOutInENotation() {
        // "4.0E-4" was read by the core as 4.0 times a point E (or "E kenne ich nicht").
        assertEquals("0.0004", GraphSession.num(0.0004))
        assertEquals("-0.0004", GraphSession.num(-0.0004))
        assertEquals("15000000", GraphSession.num(1.5e7))
        assertEquals("1.5", GraphSession.num(1.5))
        assertEquals("3", GraphSession.num(3.0000000001))
        assertEquals("0.333333", GraphSession.num(1.0 / 3))
        assertEquals("0", GraphSession.num(1e-9))
        assertEquals("0", GraphSession.num(-0.0))
        assertEquals("12345678901234", GraphSession.num(12345678901234.0))
        assertEquals("0", GraphSession.num(Double.NaN))
    }

    @Test fun theRotationAngleIsOneNumber() {
        // ° binds to the number before it only: 180/4 became 180/(4°); "90°" became "90°°".
        assertEquals("Drehe(A, (180/4)°, M)", GTool.ROTATE.command(listOf("A", "M", "180/4")))
        assertEquals("Drehe(A, (90)°, M)", GTool.ROTATE.command(listOf("A", "M", "90°")))
    }

    @Test fun decimalCommasButNotArgumentCommas() {
        assertEquals("1.5", GraphSession.decimalPoints("1,5"))
        assertEquals("Abstand(A, B)", GraphSession.decimalPoints("Abstand(A, B)"))
        assertEquals("f(2.5, 3)", GraphSession.decimalPoints("f(2,5, 3)"))
    }

    @Test fun everyToolOnTheBarHasItsOwnName() {
        // "Kreis" three times, "Spiegeln" twice.
        val names = GTool.values().map { it.short }
        assertEquals(names.groupBy { it }.filterValues { it.size > 1 }.keys, emptySet<String>())
    }

    @Test fun aThousandBracketsAreRefusedBeforeTheCoreReadsThem() {
        // The core's reader recurses without a limit: drawing such a graph overflowed the stack.
        assertEquals(null, GraphSession.tooComplex("f(x) = sin((x + 1)^2) / (2 - x)"))
        assertEquals("Zu tief verschachtelt", GraphSession.tooComplex("(".repeat(500) + "x" + ")".repeat(500)))
        assertEquals("Zu tief verschachtelt", GraphSession.tooComplex("-".repeat(500) + "x"))
        assertEquals(null, GraphSession.tooComplex("a - - b"))
        assertEquals(true, GraphSession.tooComplex("x+".repeat(1500) + "1")?.startsWith("Zu lang"))
    }
}
