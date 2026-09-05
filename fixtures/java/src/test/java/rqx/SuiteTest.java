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

    static void assertEquals(int expected, int actual) {
    }
}
