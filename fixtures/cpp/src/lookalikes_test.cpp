// Code that resembles a finding and is not one. Every rule should stay silent here.
#include "lookalikes.hpp"

// self-asserting-test: each side runs the code, so the check holds it to answering the
// same twice.
TEST(Lookalikes, ScalesTheSameValueTheSameWayTwice) {
    EXPECT_EQ(scaled(1), scaled(1));
}
