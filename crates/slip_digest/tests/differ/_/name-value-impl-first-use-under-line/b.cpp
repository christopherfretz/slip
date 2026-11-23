// WP-115 fix round, F1 (digest §9): u1 and u3 share one object (#line puts both at line 50, column 1); pick returns u2 on side A, u3 on side B: an object is written as its first use's ordinal, so K differs
#include "srcloc.h"
using SL = std::source_location;
consteval SL pick(SL a, SL b, SL c) { return a.line() == c.line() ? b : c; }
constexpr SL K = pick(

SL::current(),
SL::current(),

SL::current());
