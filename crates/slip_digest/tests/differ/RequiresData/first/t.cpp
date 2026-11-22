#include "side.h"
// §9.3 RequiresData `first`: a requirement added.
#ifdef B
template<class T> bool rv = requires(T t) { t + 1; t - 1; };

#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
