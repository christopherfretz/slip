// WP-115 item 2 (digest §9): two uses in one specialisation's initializer (V<0>) are two objects, by their ordinal there: G passing the first against the second (chosen by its line) differs
#include "srcloc.h"
template <int N> constexpr std::source_location V[2] = {std::source_location::current(), std::source_location::current()};

constexpr std::source_location G = V<0>[__builtin_LINE() % 2];
