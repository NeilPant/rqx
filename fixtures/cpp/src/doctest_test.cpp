#include <doctest/doctest.h>

TEST_SUITE("quality examples") {
    TEST_CASE("an empty doctest") {
    }

    TEST_CASE("does work without a check") {
        int value = add(1, 2);
        (void)value;
    }

    TEST_CASE("checks the result") {
        CHECK(add(1, 2) == 3);
    }

    TEST_CASE("has subcases") {
        SUBCASE("an empty subcase") {
        }
        SUBCASE("checks a subcase") {
            CHECK(add(1, 2) == 3);
        }
    }
}
