package dev.n0bridge.n0te

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where a page's ink is put into cells and which tiles a view needs. */
class ChunkPlanTest {
    private val cell = ChunkPlan.CELL.toFloat()

    /** [n] letters in no order on the page (as the core hands them out, by id): a dense 2000 × 2000 pt page. */
    private fun scattered(n: Int): FloatArray {
        val rnd = java.util.Random(7)
        val b = FloatArray(4 * n)
        for (i in 0 until n) {
            val x = rnd.nextFloat() * 2000f; val y = rnd.nextFloat() * 2000f
            b[4 * i] = x; b[4 * i + 1] = y; b[4 * i + 2] = x + 10f; b[4 * i + 3] = y + 18f
        }
        return b
    }

    private fun cellsOf(n: Int, b: FloatArray) = ChunkPlan.plan(n, b).mapValues { it.value.toList() }

    @Test fun strokesGoIntoTheCellsTheyReachInTheirOrder() {
        val b = floatArrayOf(
            10f, 10f, 20f, 20f, // 0: in cell (0, 0)
            cell - 2f, 10f, cell + 30f, 20f, // 1: across the edge: (0, 0) and (1, 0)
            cell + 50f, cell + 50f, cell + 60f, cell + 60f, // 2: (1, 1)
            -40f, -40f, -30f, -30f, // 3: left of and above the page (a board): (-1, -1)
            100f, 100f, 100f, 100f, // 4: a dot: one cell
        )
        val p = cellsOf(5, b)
        assertEquals(listOf(0, 1, 4), p[ChunkPlan.key(0, 0)])
        assertEquals(listOf(1), p[ChunkPlan.key(1, 0)])
        assertEquals(listOf(2), p[ChunkPlan.key(1, 1)])
        assertEquals(listOf(3), p[ChunkPlan.key(-1, -1)])
        assertEquals(4, p.size)
        assertEquals(-1, ChunkPlan.col(ChunkPlan.key(-1, 7)))
        assertEquals(7, ChunkPlan.row(ChunkPlan.key(-1, 7)))
        // Near an edge counts as in both (its anti-aliased rim reaches across).
        assertArrayEquals(intArrayOf(0, 0, 1, 0), ChunkPlan.cells(cell - 10f, 10f, cell - ChunkPlan.REACH / 2, 20f))
    }

    @Test fun cellsStaySmallHoweverTheStrokesAreOrdered() {
        val n = 20_000
        val p = cellsOf(n, scattered(n))
        // 2000 pt across: 8 × 8 cells (and a ninth row/column for the rims).
        assertTrue(p.size in 64..81)
        val total = p.values.sumOf { it.size }
        assertTrue("strokes counted twice at edges: $total", total < n * 1.3)
        for (list in p.values) assertTrue("in order", list.zipWithNext().all { (a, c) -> c > a })
    }

    @Test fun aNewOrErasedStrokeOnlyChangesItsCells() {
        val n = 5000
        val b = scattered(n)
        val before = cellsOf(n - 1, b.copyOf(4 * (n - 1)))
        val after = cellsOf(n, b)
        val changed = after.keys.filter { after[it] != before[it] }
        assertEquals(ChunkPlan.cells(b[4 * (n - 1)], b[4 * (n - 1) + 1], b[4 * (n - 1) + 2], b[4 * (n - 1) + 3]).let { (it[2] - it[0] + 1) * (it[3] - it[1] + 1) }, changed.size)
        // Erasing stroke 1234: its cells change (the others only see their indices shift by one).
        val keep = (0 until n).filter { it != 1234 }
        val b2 = FloatArray(4 * keep.size).also { o -> keep.forEachIndexed { j, i -> for (q in 0..3) o[4 * j + q] = b[4 * i + q] } }
        val erased = cellsOf(keep.size, b2).mapValues { e -> e.value.map { keep[it] } }
        val differ = after.keys.filter { after[it] != erased[it] }
        assertTrue(differ.isNotEmpty() && differ.all { k -> after[k]!!.contains(1234) })
    }

    @Test fun onlyCellsInViewShow() {
        assertTrue(ChunkPlan.shows(0f, 0f, 10f, 10f, 5f, 5f, 20f, 20f))
        assertFalse(ChunkPlan.shows(0f, 0f, 10f, 10f, 10f, 0f, 20f, 20f))
        assertFalse(ChunkPlan.shows(0f, 30f, 10f, 40f, 0f, 0f, 20f, 20f))
        assertTrue("a view inside a cell", ChunkPlan.shows(0f, 0f, 100f, 100f, 40f, 40f, 50f, 50f))
        assertFalse("nothing in view", ChunkPlan.shows(-3f, -3f, 10f, 10f, 0f, 0f, 0f, 0f))
    }

    @Test fun tilesAreDrawnAtStepsOfZoomNeverBlownUp() {
        // Steps of √2: a tile serves every zoom up to its own and is at most √2 too sharp.
        for (s in listOf(0.013f, 0.27f, 0.5f, 0.7072f, 1f, 1.3f, 3.9f, 31f)) {
            val l = ChunkPlan.level(s)
            assertTrue("$s → $l", l >= s * 0.9999f && l < s * 1.4143f)
            assertEquals(l, ChunkPlan.level(l * 0.98f), l * 1e-5f)
        }
        assertEquals(1f, ChunkPlan.level(1f), 1e-6f)
        assertEquals(0.5f, ChunkPlan.level(0.5f), 1e-6f)
        // The tiles over a view: whole tiles of 512 px at the level, also left of and above the origin.
        assertArrayEquals(intArrayOf(0, 0, 1, 0), ChunkPlan.tiles(0f, 0f, 1000f, 512f, 1f))
        assertArrayEquals(intArrayOf(-1, -2, 0, 0), ChunkPlan.tiles(-10f, -600f, 512f, 10f, 1f))
        // 5670 pt shown whole at 0.27 px/pt: 1531 px, 3 tiles across.
        assertArrayEquals(intArrayOf(0, 0, 2, 2), ChunkPlan.tiles(0f, 0f, 5670f, 5670f, 0.27f))
    }
}
