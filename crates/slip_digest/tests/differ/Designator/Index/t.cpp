#include "side.h"
// WP-035, digest-kinds §7 `Designator` `Index`.
#ifdef B
template <class T> struct A { static inline T w[3] = {[0 ... 0] = 1}; };
#else
template <class T> struct A { static inline T w[3] = {[0] = 1}; };
#endif
