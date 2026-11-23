// std: c++17
#include "side.h"
// §4.3 AutoKeyword `GnuAutoType`: `__auto_type` against `auto`, undeduced in a template.
#ifdef B
template <class T> __auto_type v = T();
#else
template <class T> auto v = T();
#endif
