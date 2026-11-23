// WP-115 item 2 (digest §9): two aggregate initializations using one default member initializer in one holder (s2) are two objects, by their ordinal there: G passing the first against the second (chosen by its line) differs
#include "srcloc.h"
struct S { std::source_location l = std::source_location::current(); };
constexpr S s2[2] = {S{}, S{}};
constexpr std::source_location G = s2[__builtin_LINE() % 2].l;
