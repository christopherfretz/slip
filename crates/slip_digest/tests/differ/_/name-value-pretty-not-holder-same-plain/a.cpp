// WP-115 item 1 (digest §9): x::n() and y::n() share the plain string "n" but not their pretty names; G, no function, passing one pretty name or the other differs
#include "srcloc.h"
namespace x { consteval const char *n() { return std::source_location::current().function_name(); } }
namespace y { consteval const char *n() { return std::source_location::current().function_name(); } }
consteval const char *pick(int i) { return i % 2 ? x::n() : y::n(); }
constexpr const char *G = pick(__builtin_LINE());
