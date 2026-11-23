// WP-115 fix round, F1 (digest §9): u1 and u2 share one __impl object on side A (one macro invocation); K is that object, written as the ordinal of its first use, u1: the same as side B's u1
#include "srcloc.h"
#define BOTH(...) __VA_ARGS__
using SL = std::source_location;
consteval SL first(SL a, SL b) { return a; }
constexpr SL K = first(BOTH(SL::current(), SL::current()));
