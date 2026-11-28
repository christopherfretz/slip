#include "side.h"
// WP-136 item 4 ("Its kin"): a class template partial specialisation that
// did not match on side A (Kiln keeps no failed declaration) and matches on
// side B.
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { using type = void; };
#else
template <> struct Trait<int> {};
#endif
template <class T, class = void> struct P { static constexpr int v = 1; };
template <class T> struct P<T, typename Trait<T>::type> { static constexpr int v = 2; };
int u() {
    char buf[P<int>::v];
    return sizeof(buf);
}
