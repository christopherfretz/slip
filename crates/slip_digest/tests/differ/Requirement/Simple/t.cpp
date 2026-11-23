#include "side.h"
// §9.3 Requirement `Simple`: a simple requirement against a nested one of the same expression.
#ifdef B
template<class T> bool rv = requires(T t) { requires true; };

#else
template<class T> bool rv = requires(T t) { true; };

#endif
