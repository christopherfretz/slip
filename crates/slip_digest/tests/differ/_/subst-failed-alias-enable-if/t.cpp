#include "side.h"
// WP-136 item 4 ("Its kin"): alias template (`enable_if_t<false>`) whose
// substitution fails inside another function template's signature.
template <bool, class T = void> struct enable_if {};
template <class T> struct enable_if<true, T> { using type = T; };
template <bool Cond, class T = void> using enable_if_t = typename enable_if<Cond, T>::type;
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { static constexpr bool ok = true; };
#else
template <> struct Trait<int> { static constexpr bool ok = false; };
#endif
template <class T> enable_if_t<Trait<T>::ok, int> f(T);
long f(...);
long u() { return f(1); }
