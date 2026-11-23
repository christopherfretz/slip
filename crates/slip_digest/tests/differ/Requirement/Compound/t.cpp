#include "side.h"
// §9.3 Requirement `Compound`: a compound requirement against a simple one.
#ifdef B
template<class T> bool rv = requires(T t) { t + 1; };

#else
template<class T> bool rv = requires(T t) { { t + 1 }; };

#endif
