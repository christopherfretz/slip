#include "side.h"
// digest §3.4, WP-039 item 1: function templates differing only in a decltype operand: two keys.
template <class T> auto f(T t) -> decltype(t.a);
#ifdef B
template <class T> auto f(T t) -> decltype(t.b);
#endif
