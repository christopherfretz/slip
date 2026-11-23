#include "side.h"
// §9.3 Requirement `Type`: a type requirement against a simple one.
#ifdef B
template<class T> bool rv = requires(T t) { T::type; };

#else
template<class T> bool rv = requires(T t) { typename T::type; };

#endif
