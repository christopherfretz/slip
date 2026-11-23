#include "side.h"
// §9.3 Requirement `Simple.0`.
#ifdef B
template<class T> bool rv = requires(T t) { t + 2; };

#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
