#include "side.h"
// §9.3 Requirement `Nested`: a nested requirement against a simple one.
#ifdef B
template<class T> bool rv = requires(T t) { true; };

#else
template<class T> bool rv = requires(T t) { requires true; };

#endif
