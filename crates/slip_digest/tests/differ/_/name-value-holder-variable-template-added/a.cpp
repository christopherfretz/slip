// WP-115 item 2 (digest §9): a use in a variable template's initializer is numbered among its specialisation's uses: another specialisation used before it does not renumber V<0>'s
#include "srcloc.h"
template <int N> constexpr std::source_location V[2] = {std::source_location::current(), std::source_location::current()};
constexpr int other = 0;
constexpr std::source_location G = V<0>[1];
