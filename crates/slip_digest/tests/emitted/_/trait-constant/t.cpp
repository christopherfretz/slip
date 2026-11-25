#include "side.h"
// WP-155: `zz153-q3`'s trait, a `static constexpr` member instantiated in
// side A only: constant-initialised, not marked.
template <class T> struct Tr { static constexpr bool value = sizeof(T) > 0; };
#ifndef B
static_assert(Tr<int>::value);
#endif
