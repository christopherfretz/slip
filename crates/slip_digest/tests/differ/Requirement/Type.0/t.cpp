#include "side.h"
// §9.3 Requirement `Type.0`.
#ifdef B
template<class T> bool rv = requires(T t) { typename T::other; };

#else
template<class T> bool rv = requires(T t) { typename T::type; };

#endif
