#include "side.h"
// §9.3 Requirement `Compound.constraint`: a return-type-requirement added.
template<class T> concept C = true;
#ifdef B
template<class T> bool rv = requires(T t) { { t + 1 } -> C; };

#else
template<class T> bool rv = requires(T t) { { t + 1 }; };

#endif
