// WP-115 item 2 (digest §9): the first use of g1 and the first use of g2 have one ordinal each in its own holder; G passing one or the other (chosen by its line) differs by the holder written with the ordinal
#include "srcloc.h"
constexpr std::source_location g1 = std::source_location::current();
constexpr std::source_location g2 = std::source_location::current();
constexpr std::source_location G = __builtin_LINE() % 2 ? g1 : g2;
