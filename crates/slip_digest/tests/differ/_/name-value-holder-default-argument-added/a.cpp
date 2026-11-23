// WP-115 item 2 (digest §9): a use of current()'s default argument in a namespace-scope initializer is numbered among its holder's (the variable): a use added to another holder (g1) does not renumber g2's
#include "srcloc.h"
constexpr unsigned g1 = 0;
constexpr std::source_location g2[2] = {std::source_location::current(), std::source_location::current()};
constexpr std::source_location G = g2[1];
