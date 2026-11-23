#include "side.h"
// digest §3.4, WP-039 item 1: variable template partial specialisations: two keys.
template <class T> int v = 0;
template <class T> int v<T *> = 1;
#ifdef B
template <class T> int v<T &> = 2;
#endif
