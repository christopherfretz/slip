#include "side.h"
// WP-136 item 4 ("Its kin"): failed substitution of a deduction guide.
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { using type = long; };
#else
template <> struct Trait<int> {};
#endif
template <class T> struct A { A(T); };
template <class T> A(T) -> A<typename Trait<T>::type>;
A a(1);
