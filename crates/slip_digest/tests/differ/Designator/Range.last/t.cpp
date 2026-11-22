#include "side.h"
// WP-035, digest-kinds §7 `Designator` `Range.last`.
#ifdef B
template <class T> struct A { static inline T w[3] = {[0 ... 2] = 1}; };
#else
template <class T> struct A { static inline T w[3] = {[0 ... 1] = 1}; };
#endif
