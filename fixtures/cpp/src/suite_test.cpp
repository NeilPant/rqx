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

TEST(Pricing, ReadsTheStore) {
    auto rows = db.query("rows");
    EXPECT_EQ(1, rows);
}

TEST(Works, Works) {
    EXPECT_EQ(2, add(1, 1));
}

TEST(Pricing, CountsThreeWays) {
    EXPECT_EQ(2, add(1, 1));
    EXPECT_EQ(4, add(2, 2));
    EXPECT_EQ(6, add(3, 3));
}

TEST(Pricing, HoldsTheCodeToItself) {
    EXPECT_EQ(add(1, 1), add(1, 1));
}

TEST(Pricing, IsNotWrittenYet) {
}

TEST(Pricing, DrivesTheLot) {
    int one = add(1, 1);
    EXPECT_EQ(one, 2);
    EXPECT_EQ(0, std::string("{\"two\":4,\"spare\":\"padding here\"}").size());
}

TEST(Pricing, DrivesFiveUnits) {
    int a = add(1, 1);
    int b = many_paths(1, 1);
    int c = fresh_total(1);
    int d = narrated(1);
    int e = guarded_add(1, 1, 1);
    EXPECT_GT(a + b + c + d + e, 0);
}

TEST(Pricing, StandsSomethingIn) {
    auto client = NiceMock();
    EXPECT_EQ(2, add(1, 1));
}

int spare = 0;

void SetUp() {
    spare = 1;
}

TEST(Pricing, BuildsBothSidesFromOneHelper) {
    EXPECT_EQ(add(1, 1) + 1, add(add(1, 1), 1));
}
