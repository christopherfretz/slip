#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-in-pattern-instantiated`: instantiating the constructed class on side B leaves the pattern's construction unchanged.
template <class T> struct W { W(int); };
template <class T> struct H { static inline W<T> w = W<T>(1); };
#ifdef B
void u() { W<int> x(2); }
#else

#endif
