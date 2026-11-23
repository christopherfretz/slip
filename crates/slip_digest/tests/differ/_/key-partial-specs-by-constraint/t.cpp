#include "side.h"
// digest §3.4, WP-039 item 1: partial specialisations differing only in a requires-clause: two keys.
template <class T> concept C1 = sizeof(T) == 1;
template <class T> concept C2 = sizeof(T) == 2;
template <class T> struct S {};
template <class T> requires C1<T> struct S<T *> {};
#ifdef B
template <class T> requires C2<T> struct S<T *> {};
#endif
