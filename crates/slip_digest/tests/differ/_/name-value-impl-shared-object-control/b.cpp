// WP-115 fix round, F1 control (digest §9): no sharing; K is u3 on side A, u2 on side B
#include "srcloc.h"
using SL = std::source_location;
consteval SL pick(SL a, SL b, SL c, bool x) { return x ? c : b; }
constexpr SL K = pick(SL::current(), SL::current(), SL::current(), false);
