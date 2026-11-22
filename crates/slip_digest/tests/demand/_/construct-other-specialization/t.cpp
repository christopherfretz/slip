#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-other-specialization`: instantiating another specialization of the constructed class template on side B leaves the construction unchanged.
template <class T> struct W { W(int); };
W<int> v(1);
#ifdef B
W<long> u(2);
#else

#endif
