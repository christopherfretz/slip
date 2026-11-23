// WP-115 item 2 (digest §9): two init captures' uses in one holder (l) are two objects, by their ordinal there: G passing the first against the second differs
#include "srcloc.h"
constexpr auto l = [x = std::source_location::current(), y = std::source_location::current()] { return y; };
constexpr std::source_location G = l();
