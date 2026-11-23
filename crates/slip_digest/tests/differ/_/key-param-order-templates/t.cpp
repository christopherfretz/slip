#include "side.h"
// digest §3.4, WP-039 item 1: function templates whose parameters swap: two keys.
template <class T, class U> void f(T, U);
#ifdef B
template <class T, class U> void f(U, T);
#endif
