// Functions copied unchanged from https://github.com/fmtlib/fmt at commit 14ac0d5a9d40aef14f50954069c1e10971abb0b2.
// MIT. Copyright (c) 2012 - present, Victor Zverovich and {fmt} contributors.
// The license text is in ../NOTICE. Names a function uses that the project defines elsewhere are left undefined. The functions are unchanged; the includes and the constants they use are declared above them.

#include <cstdint>
#include <cstring>

constexpr int work_bits = 320;                  // internal working precision
constexpr int work_limbs = work_bits / 64;      // working limbs (5)

// From fmtlib/fmt include/fmt/format-inl.h:167 at 14ac0d5a9d40.
// @complexity time=O(n*m) space=O(1) conf=verified
inline void mul(const uint64_t* a, int na, const uint64_t* b, int nb,
                uint64_t* out) noexcept {
  memset(out, 0, size_t(na + nb) * sizeof(*out));
  for (int i = 0; i < na; ++i) {
    uint64_t carry = 0;
    for (int j = 0; j < nb; ++j) {
      uint128_t p = umul128(a[i], b[j]);
      uint64_t lo = uint64_t(p), hi = uint64_t(p >> 64);
      uint64_t s = out[i + j] + lo;
      uint64_t c = uint64_t(s < lo);
      s += carry;
      c += uint64_t(s < carry);
      out[i + j] = s;
      carry = hi + c;
    }
    out[i + nb] = carry;  // this limb is still zero: prior rows stop at i+nb-1
  }
}

// From fmtlib/fmt include/fmt/format-inl.h:187 at 14ac0d5a9d40.
// @complexity time=O(n) space=O(1) conf=verified
inline void extract(const uint64_t* a, int n, int shift, uint64_t* out,
                    int outn) noexcept {
  int ws = shift >> 6, bs = shift & 63;
  for (int i = 0; i < outn; ++i) {
    uint64_t lo = ws + i < n ? a[ws + i] : 0;
    uint64_t hi = ws + i + 1 < n ? a[ws + i + 1] : 0;
    out[i] = bs != 0 ? (lo >> bs | hi << (64 - bs)) : lo;
  }
}

// From fmtlib/fmt include/fmt/format-inl.h:198 at 14ac0d5a9d40.
// @complexity time=O(1) space=O(1) conf=verified
inline auto fmul(const uint64_t* a, const uint64_t* b, uint64_t* r) noexcept
    -> int {
  uint64_t prod[2 * work_limbs];
  mul(a, work_limbs, b, work_limbs, prod);
  // Both operands are normalized, so the product's MSB is bit 638 or 639.
  int shift = work_bits - 1 + int(prod[2 * work_limbs - 1] >> 63);
  extract(prod, 2 * work_limbs, shift, r, work_limbs);
  return shift;
}

// From fmtlib/fmt include/fmt/format-inl.h:211 at 14ac0d5a9d40.
// @complexity time=O(log n) space=O(1) conf=verified none=repeated-computation-in-loop
inline auto fpow(uint64_t* base, int base_exp, int n, uint64_t* r) noexcept
    -> int {
  memset(r, 0, work_limbs * sizeof(*r));
  r[work_limbs - 1] = uint64_t(1) << 63;  // 1.0
  int rexp = 1 - work_bits;
  while (n != 0) {
    if ((n & 1) != 0) rexp += base_exp + fmul(r, base, r);
    n >>= 1;
    if (n != 0) base_exp = 2 * base_exp + fmul(base, base, base);
  }
  return rexp;
}

// From fmtlib/fmt include/fmt/format.h:2294 at 14ac0d5a9d40.
// @complexity time=O(1) space=O(1) conf=verified
FMT_CONSTEXPR inline auto display_width_of(uint32_t cp) noexcept -> size_t {
  if (cp < 0x1100) return 1;
  size_t lo = 0;
  size_t hi = sizeof(wide_cp_data<>::ranges) / sizeof(wide_cp_range);
  while (lo < hi) {
    size_t mid = lo + (hi - lo) / 2;
    if (cp < wide_cp_data<>::ranges[mid].first)
      hi = mid;
    else if (cp > wide_cp_data<>::ranges[mid].last)
      lo = mid + 1;
    else
      return 2;
  }
  return 1;
}

// From fmtlib/fmt include/fmt/format.h:796 at 14ac0d5a9d40.
// @complexity time=O(n) space=O(1) conf=verified
inline FMT_ZMIJ_CONSTEXPR20 auto copy_n(char* out, const char* in,
                                        size_t count) noexcept -> char* {
  for (size_t i = 0; i < count; ++i) *out++ = *in++;
  return out;
}
