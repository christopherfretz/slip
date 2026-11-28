#include "side.h"
// WP-136 items 1 & 5 (`spec/digest.md` §2.1, §8): a failing candidate tried
// on side A (via ADL) and never tried on side B (qualified call) produces
// identical unit sets: a failed specialisation is no unit.
namespace a {
struct S {};
template <class T> auto g(T t) -> decltype(t.size());
}
int g(...);
int u() {
#ifdef B
    return ::g(a::S{});
#else
    return g(a::S{});
#endif
}
