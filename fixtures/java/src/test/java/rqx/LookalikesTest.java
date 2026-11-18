package rqx;

import org.junit.jupiter.api.Test;

// Code that resembles a finding and is not one. Every rule should stay silent here.
public class LookalikesTest {
    // self-asserting-test: each side runs the code, so the check holds it to answering
    // the same twice.
    @Test
    public void testScalesTheSameValueTheSameWayTwice() {
        assertEquals(new Lookalikes().scaled(1), new Lookalikes().scaled(1));
    }
}
