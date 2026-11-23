// WP-115 fix round, F1 (digest §9): u1 and u2 share one __impl object (one macro invocation: same line and column); K is u3 on side A and u2 on side B; every use is numbered, so u3 is ordinal 2 and u2 ordinal 1 whatever objects Kiln shares
#include "srcloc.h"
#define BOTH(...) __VA_ARGS__
using SL = std::source_location;
consteval SL pick(SL a, SL b, SL c) { return a.line() == b.line() && a.column() == b.column() ? c : b; }
constexpr SL K = pick(SL::current(), SL::current(), SL::current());
