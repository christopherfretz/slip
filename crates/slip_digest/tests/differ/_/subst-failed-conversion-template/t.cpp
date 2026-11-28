#include "side.h"
// WP-136 item 4 ("Its kin"): failed substitution of a conversion function
// template (`explicit(expr)` fails on side A, succeeds on side B).
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { static constexpr bool value = false; };
#else
template <> struct Trait<int> {};
#endif
struct C {
    template <class T> explicit(Trait<T>::value) operator T() const;
    operator long() const;
};
int u(C c) {
    int x = c;
    return x;
}
