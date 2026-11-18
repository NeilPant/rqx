#include "clean.hpp"
#include <stdexcept>

static void sum_is(int got, int want) {
    EXPECT_GE(got, 0);
    EXPECT_EQ(got, want);
}

TEST(Checks, ComparesWithGoogleTest) {
    EXPECT_NE(add(1, 1), 3);
}

TEST(Checks, ExpectsAThrow) {
    EXPECT_THROW(add(1, 1), std::runtime_error);
}

TEST(Checks, ExpectsAThrownPointer) {
    ASSERT_THROW(add(1, 1), const char*);
}

TEST(Checks, ChecksAtCompileTime) {
    static_assert(sizeof(int) >= 2, "int holds two bytes");
}

TEST(Checks, ChecksTheCatchWay) {
    REQUIRE(add(1, 1) == 2);
}

TEST(Checks, FailsByHand) {
    if (add(1, 0) != 1) {
        FAIL() << "add(1, 0) is not 1";
    }
}

TEST(Checks, ChecksThroughAHelper) {
    sum_is(add(1, 1), 2);
}

TEST(Checks, RunsATable) {
    for (const int number : {0, 1}) {
        EXPECT_EQ(add(number, 0), number);
    }
}

TEST(Checks, DecidesThreeWays) {
    int total = add(0, 1);
    if (total > 1) {
        total = add(total, -1);
    } else {
        total = add(total, 1);
    }
    while (total < 2) {
        total = add(total, 1);
    }
    switch (total) {
    case 2:
        break;
    default:
        total = 0;
    }
    EXPECT_EQ(total, 2);
}
