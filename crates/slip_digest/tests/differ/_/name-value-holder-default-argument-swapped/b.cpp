// WP-115 item 2 (digest §9): two uses of current()'s default argument in one holder (g2) are two objects, by their ordinal there: G passing the first against the second (chosen by its line) differs
#include "srcloc.h"
constexpr std::source_location g2[2] = {std::source_location::current(), std::source_location::current()};

constexpr std::source_location G = g2[__builtin_LINE() % 2];
