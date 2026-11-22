#include "side.h"
// WP-035, digest-kinds §7 `Designator` `Field.name`: the member is looked up by name at instantiation.
struct P { int x, y; };
#ifdef B
template <class T> struct A { static inline T w = {.y = 1}; };
#else
template <class T> struct A { static inline T w = {.x = 1}; };
#endif
