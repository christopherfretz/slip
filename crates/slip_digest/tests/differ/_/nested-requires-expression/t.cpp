#include "side.h"
// §4.4, §7 _ `nested-requires-expression`: a requires-expression nested in a requirement.
#ifdef B
template<class T> bool rv = requires(T t) { requires requires(T u) { u + 2; }; };

#else
template<class T> bool rv = requires(T t) { requires requires(T u) { u + 1; }; };

#endif
