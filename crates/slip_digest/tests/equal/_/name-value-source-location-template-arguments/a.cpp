// WP-104 (digest §9), two std::source_location template arguments name two specialisations (a key holds the value by value)
#include "srcloc.h"
template <std::source_location L> struct C { static constexpr int v = 2; };
int g() { return C<std::source_location::current()>::v + C<std::source_location::current()>::v; }
