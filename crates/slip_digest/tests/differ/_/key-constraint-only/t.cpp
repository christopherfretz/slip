#include "side.h"
// digest §3.4, WP-039 item 1: overloads differing only in a trailing requires-clause: two keys.
template <class T> concept C1 = sizeof(T) == 1;
template <class T> concept C2 = sizeof(T) == 2;
template <class T> void f(T) requires C1<T>;
#ifdef B
template <class T> void f(T) requires C2<T>;
#endif
