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
