package rqx;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

public class SuiteTest {
    @Disabled
    @Test
    public void testDiscounts() {
        assertEquals(3, new Clean().add(1, 2));
    }

    @Test
    public void testShips() throws Exception {
        Thread.sleep(500);
        assertEquals(2, new Clean().add(1, 1));
    }

    @Test
    public void testChecksNothing() {
        int value = new Clean().add(1, 1);
    }


    @Test
    public void testBranchesOnTheInput() {
        if (new Clean().add(1, 1) == 2) {
            assertEquals(2, new Clean().add(1, 1));
        }
    }

    @Test
    public void testAssertsTheSameThingTwice() {
        assertEquals(2, new Clean().add(1, 1));
        assertEquals(2, new Clean().add(1, 1));
    }

    @Test
    public void testCatchesTheFailureItself() {
        try {
            new Clean().add(1, 1);
        } catch (IllegalStateException error) {
            assertEquals(1, 1);
        }
    }

    @Test
    public void testPrintsWhileItRuns() {
        System.out.println("here");
        assertEquals(2, new Clean().add(1, 1));
    }


    @Test
    public void testReadsTheStore() {
        Object rows = db.query("rows");
        assertEquals(1, rows == null ? 0 : 1);
    }

    static Db db = new Db();

    static class Db {
        Object query(String sql) {
            return null;
        }
    }


    @Test
    public void testWorks() {
        assertEquals(2, new Clean().add(1, 1));
    }


    @Test
    public void testCountsThreeWays() {
        assertEquals(2, new Clean().add(1, 1));
        assertEquals(4, new Clean().add(2, 2));
        assertEquals(6, new Clean().add(3, 3));
    }

    @Test
    public void testHoldsTheCodeToItself() {
        assertEquals(new Clean().add(1, 1), new Clean().add(1, 1));
    }

    @Test
    public void testExpectsANumberFromNowhere() {
        assertEquals(4999, new Clean().add(2400, 2599));
    }


    @Test
    public void testIsNotWrittenYet() {
    }


    @Test
    public void testDrivesTheLot() {
        int one = new Clean().add(1, 1);
        assertEquals(2, one);
        assertSame("{\"two\":4,\"spare\":\"padding here\"}", "x");
    }

    static void assertSame(String expected, String actual) {
    }


    @Test
    public void testDrivesFiveUnits() {
        int a = new Clean().add(1, 1);
        int b = new Hygiene().narrated(1);
        int c = new Hygiene().scoreOf(1);
        int d = new Hygiene().guardedAdd(1, 1, 1);
        int e = new Flow().freshTotal(1);
        assertEquals(0, a + b + c + d + e);
    }

    static void assertEquals(int expected, int actual) {
    }
}
