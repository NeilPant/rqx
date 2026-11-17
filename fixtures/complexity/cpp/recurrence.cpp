// Recursion, one shape each. Their cost needs the call graph, so none is marked local.
#include <unordered_map>
#include <vector>

// @complexity time=O(n) space=O(n) conf=verified
long long factorial(int n) {
    return n <= 1 ? 1 : n * factorial(n - 1);
}

// @complexity time=O(log n) space=O(log n) conf=verified
long long power(long long base, int exponent) {
    if (exponent == 0) {
        return 1;
    }
    long long half = power(base, exponent / 2);
    return exponent % 2 == 0 ? half * half : half * half * base;
}

// @complexity time=exp space=O(n) conf=verified rule=branching-recursion
long long paths(int n) {
    if (n <= 1) {
        return 1;
    }
    return paths(n - 1) + paths(n - 2) + paths(n - 3);
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
long long paths_memo(int n, std::unordered_map<int, long long>& memo) {
    if (n <= 1) {
        return 1;
    }
    auto found = memo.find(n);
    if (found != memo.end()) {
        return found->second;
    }
    long long value = paths_memo(n - 1, memo) + paths_memo(n - 2, memo);
    memo[n] = value;
    return value;
}
