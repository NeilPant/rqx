#include <catch2/catch_test_macros.hpp>

TEST_CASE("an empty case", "[quality]") {
}

TEST_CASE("does work without checking") {
    int value = add(1, 2);
    (void)value;
}

TEST_CASE("checks a value") {
    REQUIRE(add(1, 2) == 3);
}

TEST_CASE("has sections") {
    SECTION("an empty section") {
    }
    SECTION("checks a section") {
        CHECK(add(1, 2) == 3);
    }
}

TEMPLATE_TEST_CASE("checks each type", "[types]", int, double) {
    STATIC_REQUIRE(sizeof(TestType) > 0);
}
