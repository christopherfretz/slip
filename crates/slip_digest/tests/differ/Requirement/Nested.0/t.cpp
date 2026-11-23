#include "side.h"
// §9.3 Requirement `Nested.0`.
#ifdef B
template<class T> bool rv = requires(T t) { requires false; };

#else
template<class T> bool rv = requires(T t) { requires true; };

#endif
