package rqx;

import static org.assertj.core.api.Assertions.assertThat;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.fail;
import static org.mockito.Mockito.verify;

import java.util.List;
import org.junit.Test;

public class ChecksTest {
    private final Clean clean = new Clean();

    @Test
    public void testComparesWithJUnit() {
        assertTrue(clean.add(1, 1) == 2);
    }

    @Test
    public void testComparesWithAssertJ() {
        assertThat(clean.add(1, 1)).isEqualTo(2);
    }

    @Test
    public void testExpectsAThrow() {
        assertThrows(IllegalStateException.class, () -> clean.add(1, 1));
    }

    @Test(expected = IllegalStateException.class)
    public void testExpectsAThrowByAnnotation() {
        clean.add(1, 1);
    }

    @Test
    public void testFailsByHand() {
        if (clean.add(1, 0) != 1) {
            fail("add(1, 0) is not 1");
        }
    }

    @Test
    public void testVerifiesTheStandIn() {
        final Clean standIn = org.mockito.Mockito.mock(Clean.class);
        standIn.add(1, 1);
        verify(standIn).add(1, 1);
    }

    @Test
    public void testChecksThroughAHelper() {
        sumIs(clean.add(1, 1), 2);
    }

    @Test
    public void testRunsATable() {
        for (final int value : List.of(0, 1)) {
            assertTrue(clean.add(value, 0) == value);
        }
    }

    @Test
    public void testDecidesThreeWays() {
        int total = clean.add(0, 1);
        if (total > 1) {
            total = clean.add(total, -1);
        } else {
            total = clean.add(total, 1);
        }
        while (total < 2) {
            total = clean.add(total, 1);
        }
        switch (total) {
            case 2:
                break;
            default:
                total = 0;
        }
        assertThat(total).isEqualTo(2);
    }

    static void sumIs(int got, int want) {
        assertThat(got).isEqualTo(want);
    }

    static class TestDouble {
        TestDouble() {
        }
    }
}
