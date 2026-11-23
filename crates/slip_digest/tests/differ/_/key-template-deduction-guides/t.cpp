#include "side.h"
// digest §3.4, WP-039 item 1: deduction guide templates: two keys.
template <class T> struct S { S(T); };
template <class T> S(T *) -> S<T>;
#ifdef B
template <class T> S(T &) -> S<T>;
#endif
