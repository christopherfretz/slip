#include "side.h"
// digest §3.4, WP-039 item 1: function templates differing only in return type: two keys.
template <class T> int f(T);
#ifdef B
template <class T> long f(T);
#endif
