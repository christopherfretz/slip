#include "side.h"
// §9.3 RequiresData `params`: a parameter added.
#ifdef B
template<class T> bool rv = requires(T t, T u) { t + 1; };

#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
