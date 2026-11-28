#include "side.h"
// WP-136 item 5 (`spec/digest.md` §2.1, §8): a failing template candidate
// begins to succeed and loses partial ordering to a more-specialised template
// (`g(T *)` beats `g(T)` for `int *`). The using unit `u(int *)` is unchanged;
// only `Trait<int *>` changes and the newly formed specialisation appears as a
// derived unit.
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int *> { using type = int; };
#else
template <> struct Trait<int *> {};
#endif
template <class T> auto g(T t) -> typename Trait<T>::type;
template <class T> int g(T *p);
int u(int *p) { return g(p); }
