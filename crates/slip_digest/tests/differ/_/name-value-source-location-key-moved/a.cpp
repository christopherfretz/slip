// WP-104 (digest §9): a std::source_location template argument is in a key, so by value: moving it changes the specialisation
#include "srcloc.h"
template <std::source_location L> struct C { static constexpr int v = 2; };
int g() { return C<std::source_location::current()>::v; }
