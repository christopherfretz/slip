#include "side.h"
// §4.4, §7 _ `requires-parameter-renamed`: renaming a requires-expression's parameter.
#ifdef B
template<class T> bool rv = requires(T u) { u + 1; };

#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
