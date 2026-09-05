#include "clean.hpp"
#include <chrono>
#include <thread>

TEST(Pricing, Discounts) {
    GTEST_SKIP() << "pricing is being rewritten";
    EXPECT_EQ(3, add(1, 2));
}

TEST(Pricing, Ships) {
    std::this_thread::sleep_for(500);
    EXPECT_EQ(2, add(1, 1));
}

TEST(Pricing, ChecksNothing) {
    int value = add(1, 1);
}

TEST(Pricing, BranchesOnTheInput) {
    if (add(1, 1) == 2) {
        EXPECT_EQ(2, add(1, 1));
    }
}

TEST(Pricing, AssertsTheSameThingTwice) {
    EXPECT_EQ(2, add(1, 1));
    EXPECT_EQ(2, add(1, 1));
}

TEST(Pricing, CatchesTheFailureItself) {
    try {
        add(1, 1);
    } catch (const std::runtime_error& error) {
        EXPECT_EQ(1, 1);
    }
}

TEST(Pricing, PrintsWhileItRuns) {
    printf("here\n");
    EXPECT_EQ(2, add(1, 1));
}
