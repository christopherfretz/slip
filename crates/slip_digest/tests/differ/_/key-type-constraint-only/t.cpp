#include "side.h"
// digest §3.4, WP-039 item 1: overloads differing only in a type-constraint on a template parameter: two keys.
template <class T> concept C1 = sizeof(T) == 1;
template <class T> concept C2 = sizeof(T) == 2;
template <C1 T> void f(T);
#ifdef B
template <C2 T> void f(T);
#endif
