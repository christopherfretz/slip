#include "side.h"
// WP-136 item 1 (`spec/digest.md` §2.1, §8): adding a call that triggers a
// failed substitution adds no derived unit for the failed specialisation.
template <class T> auto g(T t) -> decltype(t.size());
int g(...);
#ifdef B
int u() { return g(1); }
#endif
